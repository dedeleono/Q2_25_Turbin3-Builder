import wallet from "../Turbin3-wallet.json";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults"
import { createGenericFile, createSignerFromKeypair, signerIdentity } from "@metaplex-foundation/umi"
import { irysUploader } from "@metaplex-foundation/umi-uploader-irys"

// Create a devnet connection
const umi = createUmi('https://api.devnet.solana.com');

let keypair = umi.eddsa.createKeypairFromSecretKey(new Uint8Array(wallet));
const signer = createSignerFromKeypair(umi, keypair);

umi.use(irysUploader());
umi.use(signerIdentity(signer));

(async () => {
    try {
        // Follow this JSON structure
        // https://docs.metaplex.com/programs/token-metadata/changelog/v1.0#json-structure

        const image = "https://arweave.net/HCgdHUZm3aopP8JYdqNCgaqKPRsYkTHcEve1h25qCBQK";

        const metadata = {
            name: "Gojan_Ghibli_Rug",
            symbol: "GGR",
            description: "First Gojan's rug, lmao",
            image: image,
            attributes: [
                {trait_type: 'rarity', value: 'unique'}
            ],
            properties: {
                files: [
                    {
                        type: "image/jpeg",
                        uri: "image"
                    },
                ]
            },
            creators: []
        };
        
        const myUri = await umi.uploader.uploadJson(metadata)
        console.log("Your metadata URI: ", myUri);
    }
    catch(error) {
        console.log("Oops.. Something went wrong", error);
    }
})();
