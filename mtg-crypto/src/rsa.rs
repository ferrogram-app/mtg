// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Contains the function to RSA-encrypt hashed data as expected by Telegram.
//!
//! Not intended as a general implementation of RSA.

use num_bigint::BigUint;

use crate::{aes, sha256};

/// RSA key.
pub struct Key {
    n: BigUint,
    e: BigUint,
}

impl Key {
    pub fn new(n: &str, e: &str) -> Option<Self> {
        Some(Self {
            n: BigUint::parse_bytes(n.as_bytes(), 10)?,
            e: BigUint::parse_bytes(e.as_bytes(), 10)?,
        })
    }
}

/// Increment data by 1 when interpreted as a big-endian big int.
fn increment(data: &mut [u8]) {
    let mut i = data.len() - 1;
    loop {
        let (n, overflow) = data[i].overflowing_add(1);
        data[i] = n;
        if overflow {
            if let Some(ni) = i.checked_sub(1) {
                i = ni
            } else {
                i = data.len() - 1;
            }
        } else {
            break;
        }
    }
}

/// Encrypt the given data, prefixing it with a hash before, using RSA.
///
/// # Panics
///
/// If `data` is longer than 144 bytes (a caller bug: the inner data is a fixed-size TL object).
pub fn encrypt_hashed(data: &[u8], key: &Key, random_bytes: &[u8; 224]) -> Vec<u8> {
    // https://core.telegram.org/mtproto/auth_key#41-rsa-paddata-server-public-key-mentioned-above-is-implemented-as-follows

    // data_with_padding := data + random_padding_bytes; -- where random_padding_bytes are chosen so that the resulting length of data_with_padding is precisely 192 bytes, and data is the TL-serialized data to be encrypted as before. One has to check that data is not longer than 144 bytes.
    assert!(data.len() <= 144);
    let data_with_padding = {
        let mut buffer = Vec::with_capacity(192);
        buffer.extend(data);
        buffer.extend(&random_bytes[..192 - data.len()]);
        buffer
    };

    // data_pad_reversed := BYTE_REVERSE(data_with_padding); -- is obtained from data_with_padding by reversing the byte order.
    let data_pad_reversed = data_with_padding.iter().copied().rev().collect::<Vec<u8>>();

    // a random 32-byte temp_key is generated.
    let mut temp_key: [u8; 32] = std::array::from_fn(|i| random_bytes[192 + i]);

    let key_aes_encrypted = loop {
        // data_with_hash := data_pad_reversed + SHA256(temp_key + data_with_padding); -- after this assignment, data_with_hash is exactly 224 bytes long.
        let mut data_with_hash = {
            let mut buffer = Vec::with_capacity(224);
            buffer.extend(&data_pad_reversed);
            buffer.extend(sha256!(&temp_key, &data_with_padding));
            buffer
        };

        // aes_encrypted := AES256_IGE(data_with_hash, temp_key, 0); -- AES256-IGE encryption with zero IV.
        aes::ige_encrypt(data_with_hash.as_mut(), &temp_key, &[0u8; 32]);
        let aes_encrypted = data_with_hash;

        // temp_key_xor := temp_key XOR SHA256(aes_encrypted); -- adjusted key, 32 bytes
        let temp_key_xor = {
            let mut xored = temp_key;
            xored
                .iter_mut()
                .zip(sha256!(&aes_encrypted))
                .for_each(|(a, b)| *a ^= b);
            xored
        };

        // key_aes_encrypted := temp_key_xor + aes_encrypted; -- exactly 256 bytes (2048 bits) long
        let key_aes_encrypted = {
            let mut buffer = Vec::with_capacity(256);
            buffer.extend(temp_key_xor);
            buffer.extend(aes_encrypted);
            buffer
        };

        // The value of key_aes_encrypted is compared with the RSA-modulus of server_pubkey as a big-endian 2048-bit (256-byte) unsigned integer.
        if BigUint::from_bytes_be(&key_aes_encrypted) >= key.n {
            // If key_aes_encrypted turns out to be greater than or equal to the RSA modulus, the previous steps starting from the generation of new random temp_key are repeated.
            increment(&mut temp_key);
            continue;
        }

        // Otherwise the final step is performed:
        break key_aes_encrypted;
    };

    // encrypted_data := RSA(key_aes_encrypted, server_pubkey); -- 256-byte big-endian integer is elevated to the requisite power from the RSA public key modulo the RSA modulus, and the result is stored as a big-endian integer consisting of exactly 256 bytes (with leading zero bytes if required).
    let payload = BigUint::from_bytes_be(&key_aes_encrypted);
    let encrypted = payload.modpow(&key.e, &key.n);
    let mut block = encrypted.to_bytes_be();
    while block.len() < 256 {
        block.insert(0, 0);
    }

    block
}

/// The RSA key that signs `help.configSimple` (the fallback config served through DNS, see
/// `ConnectionsManager::decodeSimpleConfig` in the official client).
pub fn config_simple_key() -> Key {
    // Built from constants, which are valid decimal numbers.
    Key::new(
        "25594673677667620809906786596201307389551981417798428001869244604459429464878069196143782435208874919356695733541988103826593317882032589410549705427376395715741122635705196279462384209474724464739808233875090021376372573984621331707619020338278677849257179003522211669653520497011026463759018571836171412743838333545643124524738668336337407784017912929013105207540563238048508839151666097149972013206030244035495945218846612846135429832280322229948223099970250174729037221177731101114517641547287444519942815225091899265154668746316731660795947127337188464441628954070640022109333834186044802246880537741218439076039",
        "65537",
    )
    .expect("valid built-in key")
}

/// Decrypts a 256-byte `help.configSimple` block: RSA public operation, then AES-256-CBC (key =
/// the first 32 bytes, IV = bytes 16..32), then a check of the SHA-256 prefix and of the length.
///
/// Returns the TL bytes (the boxed `help.configSimple` followed by padding), or `None` if the
/// input is not exactly 256 bytes or does not verify.
pub fn decrypt_config_simple(data: &[u8], key: &Key) -> Option<Vec<u8>> {
    #[allow(deprecated)] // see aes.rs
    use ::aes::Aes256;
    use ::aes::cipher::{BlockCipherDecrypt, KeyInit};

    let data: &[u8; 256] = data.try_into().ok()?;
    let x = BigUint::from_bytes_be(data);
    if x >= key.n {
        return None;
    }
    let mut block = [0u8; 256];
    let y = x.modpow(&key.e, &key.n).to_bytes_be();
    block[256 - y.len()..].copy_from_slice(&y);

    let key_bytes: [u8; 32] = block[..32].try_into().ok()?;
    let cipher = Aes256::new((&key_bytes).into());
    let mut previous: [u8; 16] = block[16..32].try_into().ok()?;
    for chunk in block[32..].as_chunks_mut::<16>().0 {
        let ciphertext = *chunk;
        cipher.decrypt_block(chunk.into());
        chunk.iter_mut().zip(previous).for_each(|(a, b)| *a ^= b);
        previous = ciphertext;
    }

    if sha256!(&block[32..240])[..16] != block[240..] {
        return None;
    }
    let len = u32::from_le_bytes(block[32..36].try_into().ok()?) as usize;
    if len == 0 || len > 208 || !len.is_multiple_of(4) {
        return None;
    }
    Some(block[36..240].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    #[test]
    fn test_rsa_encryption() {
        let key = Key::new("25342889448840415564971689590713473206898847759084779052582026594546022463853940585885215951168491965708222649399180603818074200620463776135424884632162512403163793083921641631564740959529419359595852941166848940585952337613333022396096584117954892216031229237302943701877588456738335398602461675225081791820393153757504952636234951323237820036543581047826906120927972487366805292115792231423684261262330394324750785450942589751755390156647751460719351439969059949569615302809050721500330239005077889855323917509948255722081644689442127297605422579707142646660768825302832201908302295573257427896031830742328565032949", "65537").unwrap();
        let result = encrypt_hashed(
            &hex::from_hex("955ff5a9081a8e635f5743de9b00000004453dc27100000004622f1fcb000000f7a81627bbf511fa4afef71e94a0937474586c1add9198dda81a5df8393871c8293623c5fb968894af1be7dfe9c7be813f9307789242fd0cb0c16a5cb39a8d3e"),
            &key,
            hex::from_hex("12270000635593b03fee033d0672f9afddf9124de9e77df6251806cba93482e4c9e6e06e7d44e4c4baae821aff91af44789689faaee9bdfc7b2df8c08709afe57396c4638ceaa0dc30114f82447e81d3b53edc423b32660c43a5b8ad057b64500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000007dada0920c4973913229e0f881aec7b9db0c392d34f52fb0995ea493ecb4c09d").as_slice().try_into().unwrap(),
        );
        assert_eq!(
            result,
            hex::from_hex(
                "b610642a828b4a61fe32931815cae318d311660580f1e0df768f3140f4d37dfcfcac0c2870318de4ff2d2e0e9669bcfdc0bad06cadb1b59d9726b427368a9c7b4fc0d5e7b2e99fc571968705c03acf5341fd7021bef653fa77b3776ae430e366fc46d232459ebe128b08d80e049ae579a48b56ca93b520709468587c81af96666046e9ea85091d729e921e8d8a36f57b27644052dae7387c7f4131701d59cda75251dac66c94276280ef950d3c44c21e5a2454f7da7a6818cf23ae9c490b72b2170d7cbc24f8a93db739d76f2d241c78b80123faaff3e664f074d6375d794dbf2800a0b5bb48d54eceafedfb355bfbebd287d9023264e3b53627888250787a9e"
            )
        );
    }

    #[test]
    fn increment_carries_and_wraps() {
        let mut a = [0x00, 0xff];
        increment(&mut a);
        assert_eq!(a, [0x01, 0x00]);
        let mut a = [0x01, 0x02];
        increment(&mut a);
        assert_eq!(a, [0x01, 0x03]);
        // all ones: wraps to zero, then counts once more (2^-256 case, never hit)
        let mut a = [0xff, 0xff];
        increment(&mut a);
        assert_eq!(a, [0x00, 0x01]);
    }

    #[test]
    fn output_is_always_256_bytes_even_with_leading_zeros() {
        // n just over 2^2047: many candidates are >= n and retried with an incremented temp_key;
        // results below 2^2040 need zero padding
        let n = (BigUint::from(1u8) << 2047usize) + 12345u32;
        let key = Key::new(&n.to_string(), "1").unwrap(); // e = 1 keeps it fast
        let mut short = false;
        for seed in 0..5000u32 {
            let rnd: [u8; 224] =
                std::array::from_fn(|i| (seed as usize * 31 + i * 7) as u8 ^ seed as u8);
            let out = encrypt_hashed(&[1, 2, 3], &key, &rnd);
            assert_eq!(out.len(), 256);
            short |= out[0] == 0;
        }
        assert!(short, "no seed produced a leading zero byte");
    }

    #[test]
    fn key_new_rejects_garbage() {
        assert!(Key::new("12x", "3").is_none());
        assert!(Key::new("12", "y").is_none());
    }

    fn test_key() -> Key {
        Key::new("26360479295147871824782658357491182307227121528708014011596960955894901234742328402268014007534461846441587175359765711681630519187985656428713166218544195882430183611415197510978818801471033458758082613354100358222247889266057503674521881696873629558346000131569307477882726288560062593584818776265756508366972272561340805920766845269289917316403629106226742643055365870307644783354590924378934953749617466494856748949612285348677016470393110602600407880600891558359929121830506732354632945222831189948253079003235824485756355882567858777529290924007334611720548432089925776267081896912757733805759267885119347456031", "65537").unwrap()
    }

    #[test]
    fn config_simple_block_is_decrypted_and_verified() {
        let good = hex::from_hex(
            "46e903a0f832aeaa9e20c10335432f7203a90af67e7e7ab697224814888e8e0b9003a72a0871793e350707bd49ef38bd615ba5539a008a2f4792b75fcec5a70afb07fd71dd8a07435af65b90b2f5f1f1b4c1c172633d99bd4a940fe79d22cff4afbb9540806fb27f02f3b4c96644ef72de933fa81e0f23531903dd8676801a2dc3b7191390753c9581b0fd532684ed9fcbace780bd544dcd5a228361e727a19c6f59ab5307020734a4b3297fa41f861f2bba267f39d3c31a4d99456b6d922fea2ffa1f0e5fb524b962cf5fdd23d66c4c2f5f84843fa241b0d41cd13acdcc64aebeb0ea3ad2f26c95d974c98ed9d234faaa0fcbf04d9e6d482440fb9d071fe743",
        );
        let out = decrypt_config_simple(&good, &test_key()).unwrap();
        assert_eq!(&out[..12], &[0xde, 0xad, 0xbe, 0xef].repeat(3)[..]);

        let bad_hash = hex::from_hex(
            "caeec9bc5c1c02be8f6e0cae327729c5df22ca2020ec641cae1bb6f76ae2ba7c0aa769dce9c9fde34cac37230660aa33d65779ad49135315a01cdb73e0534c51ff9adf38c032706d61eabcacaa1afba72e9fdbf1dbf01e382c0c992da127f38fcb9a9cfaca4d27ce3419edd6618749aedfaf6d20635dbb62db11a1fb1233ada9dc4505ef1b69e05ef8e54a4a7226b2e3665c4e5fb99deedbe25dbfe33b747f8a96b27601bbf860bc78a859fdd9a1d0a5721071df205ae58028f95670ae9b1cc9869696d1bdac0a499f51db05cc4340a96528c233f981b5cd140effebe2ecdfec080bbeb4e157abca4ace9ccb2ab7d10c49ebc9c7cbb75bdb0853455b7550b9df",
        );
        assert!(decrypt_config_simple(&bad_hash, &test_key()).is_none());
        assert!(decrypt_config_simple(&good[..255], &test_key()).is_none());
        assert!(decrypt_config_simple(&[0xff; 256], &test_key()).is_none());
        // the real key does not verify a block signed with another one
        assert!(decrypt_config_simple(&good, &config_simple_key()).is_none());
    }
}
