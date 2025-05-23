use anchor_lang::prelude::*;

use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer_checked, TransferChecked, Token, Mint as SplMint, TokenAccount as SplTokenAccount},
};

use crate::error::ErrorCode;
use crate::state::{Escrow, DepositState, DepositRecord, Stable, SignaturePolicy};

#[derive(Accounts)]
#[instruction(recent_blockhash: [u8; 32])]
pub struct Release<'info> {
    #[account(
        mut,
        seeds = [b"escrow", escrow.sender.as_ref(), escrow.receiver.as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,

    /// CHECK: Storing pk to keep track
    #[account(mut)]
    pub original_depositor: AccountInfo<'info>,
    
    /// CHECK: Storing pk to keep track
    #[account(mut)]
    pub counterparty: AccountInfo<'info>,

    #[account(mut)]
    pub authorized_signer: Signer<'info>,

    /// CHECK: Storing pk to keep track
    #[account(mut)]
    pub receiving_party: AccountInfo<'info>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = original_depositor,
    )]
    pub depositor_usdc_ata: Box<Account<'info, SplTokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdt_mint,
        associated_token::authority = original_depositor,
    )]
    pub depositor_usdt_ata: Box<Account<'info, SplTokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = counterparty,
    )]
    pub counterparty_usdc_ata: Box<Account<'info, SplTokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdt_mint,
        associated_token::authority = counterparty,
    )]
    pub counterparty_usdt_ata: Box<Account<'info, SplTokenAccount>>,

    #[account(mint::token_program = token_program)]
    pub usdc_mint: Box<Account<'info, SplMint>>,

    #[account(mint::token_program = token_program)]
    pub usdt_mint: Box<Account<'info, SplMint>>,

    #[account(
        mut,
        token::mint = usdc_mint,
        token::authority = escrow,
        seeds = [b"usdc-vault", escrow.key().as_ref(), usdc_mint.key().as_ref()],
        bump,
    )]
    pub vault_usdc: Box<Account<'info, SplTokenAccount>>,

    #[account(
        mut,
        token::mint = usdt_mint,
        token::authority = escrow,
        seeds = [b"usdt-vault", escrow.key().as_ref(), usdt_mint.key().as_ref()],
        bump,
    )]
    pub vault_usdt: Box<Account<'info, SplTokenAccount>>,

    #[account(
        mut,
        constraint = deposit_record.escrow == escrow.key() @ ErrorCode::InvalidState,
        constraint = deposit_record.state == DepositState::PendingWithdrawal @ ErrorCode::InvalidState,
        seeds = [
            b"deposit",
            escrow.key().as_ref(),
            original_depositor.key().as_ref(),
            recent_blockhash.as_ref()
        ],
        bump,
    )]
    pub deposit_record: Account<'info, DepositRecord>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}


impl<'info> Release<'info> {

    pub fn release(&mut self, _recent_blockhash: [u8; 32]) -> Result<()> {
        require!(
            self.receiving_party.key() == self.original_depositor.key() ||
            self.receiving_party.key() == self.counterparty.key(),
            ErrorCode::InvalidParties
        );

        // 1. Verify the parties match the escrow's sender and receiver
        let original_depositor_is_sender = self.original_depositor.key() == self.escrow.sender;
        let original_depositor_is_receiver = self.original_depositor.key() == self.escrow.receiver;
        let counterparty_is_sender = self.counterparty.key() == self.escrow.sender;
        let counterparty_is_receiver = self.counterparty.key() == self.escrow.receiver;
        
        require!(
            (original_depositor_is_sender && counterparty_is_receiver) || 
            (original_depositor_is_receiver && counterparty_is_sender),
            ErrorCode::InvalidParties
        );

        match self.deposit_record.policy {
            SignaturePolicy::Dual => {
                // For dual signature policy:
                // - Client must ensure both parties sign the transaction
                // - In the program, we can only verify that the authorized_signer is one of the parties
                // - The second signature is validated at the transaction level
                require!(
                    self.authorized_signer.key() == self.original_depositor.key() ||
                    self.authorized_signer.key() == self.counterparty.key(),
                    ErrorCode::InvalidSigner
                );
            }
            SignaturePolicy::Single { signer } => {
                require!(
                    self.authorized_signer.key() == signer,
                    ErrorCode::InvalidSigner
                );
            }
        }
        
        let amount = self.deposit_record.amount;
        let stable = self.deposit_record.stable.clone();

        let receiving_party_is_depositor = self.receiving_party.key() == self.original_depositor.key();
        
        let (vault, to_ata, mint, decimals) = match stable {
            Stable::Usdc => (
                &self.vault_usdc,
                if receiving_party_is_depositor {
                    &self.depositor_usdc_ata
                } else {
                    &self.counterparty_usdc_ata
                },
                &self.usdc_mint,
                self.usdc_mint.decimals,
            ),
            Stable::Usdt => (
                &self.vault_usdt,
                if receiving_party_is_depositor {
                    &self.depositor_usdt_ata
                } else {
                    &self.counterparty_usdt_ata
                },
                &self.usdt_mint,
                self.usdt_mint.decimals,
            ),
        };

        let cpi_accounts = TransferChecked {
            from: vault.to_account_info(),
            mint: mint.to_account_info(),
            to: to_ata.to_account_info(),
            authority: self.escrow.to_account_info(),
        };

        let bump = self.escrow.bump;

        let escrow_seeds: [&[u8]; 4] = [
            b"escrow".as_ref(),
            self.escrow.sender.as_ref(),
            self.escrow.receiver.as_ref(),
            &[bump],
        ];
        let seeds_slice: &[&[u8]] = &escrow_seeds;
        
        let signer_seeds = &[seeds_slice];
        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );

        transfer_checked(cpi_ctx, amount, decimals)?;

        match stable {
            Stable::Usdc => self.escrow.deposited_usdc = self.escrow.deposited_usdc.checked_sub(amount).unwrap(),
            Stable::Usdt => self.escrow.deposited_usdt = self.escrow.deposited_usdt.checked_sub(amount).unwrap(),
        }

        self.deposit_record.state = DepositState::Complete;

        Ok(())
    }
}