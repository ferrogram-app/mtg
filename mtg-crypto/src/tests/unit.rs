use crate::*;

fn get_test_auth_key() -> AuthKey {
    let mut buffer = [0u8; 256];
    buffer
        .iter_mut()
        .enumerate()
        .for_each(|(i, x)| *x = i as u8);

    AuthKey::from_bytes(buffer)
}

fn get_test_msg_key() -> [u8; 16] {
    let mut buffer = [0u8; 16];
    buffer
        .iter_mut()
        .enumerate()
        .for_each(|(i, x)| *x = i as u8);

    buffer
}

fn get_test_aes_key_or_iv() -> [u8; 32] {
    let mut buffer = [0u8; 32];
    buffer
        .iter_mut()
        .enumerate()
        .for_each(|(i, x)| *x = i as u8);

    buffer
}

#[test]
fn calc_client_key() {
    let auth_key = get_test_auth_key();
    let msg_key = get_test_msg_key();
    let expected = (
        [
            112, 78, 208, 156, 139, 65, 102, 138, 232, 249, 157, 36, 71, 56, 247, 29, 189, 220, 68,
            70, 155, 107, 189, 74, 168, 87, 61, 208, 66, 189, 5, 158,
        ],
        [
            77, 38, 96, 0, 165, 80, 237, 171, 191, 76, 124, 228, 15, 208, 4, 60, 201, 34, 48, 24,
            76, 211, 23, 165, 204, 156, 36, 130, 253, 59, 147, 24,
        ],
    );
    assert_eq!(calc_key(&auth_key, &msg_key, Side::Client), expected);
}

#[test]
fn calc_server_key() {
    let auth_key = get_test_auth_key();
    let msg_key = get_test_msg_key();
    let expected = (
        [
            33, 119, 37, 121, 155, 36, 88, 6, 69, 129, 116, 161, 252, 251, 200, 131, 144, 104, 7,
            177, 80, 51, 253, 208, 234, 43, 77, 105, 207, 156, 54, 78,
        ],
        [
            102, 154, 101, 56, 145, 122, 79, 165, 108, 163, 35, 96, 164, 49, 201, 22, 11, 228, 173,
            136, 113, 64, 152, 13, 171, 145, 206, 123, 220, 71, 255, 188,
        ],
    );
    assert_eq!(calc_key(&auth_key, &msg_key, Side::Server), expected);
}

#[test]
fn encrypt_client_data_v2() {
    let mut buffer = Vec::new();
    buffer.extend(b"Hello, world! This data should remain secure!");
    let auth_key = get_test_auth_key();
    let random_padding = [0; 32];
    let expected = vec![
        50, 209, 88, 110, 164, 87, 223, 200, 168, 23, 41, 212, 109, 181, 64, 25, 162, 191, 215,
        247, 68, 249, 185, 108, 79, 113, 108, 253, 196, 71, 125, 178, 162, 193, 95, 109, 219, 133,
        35, 95, 185, 85, 47, 29, 132, 7, 198, 170, 234, 0, 204, 132, 76, 90, 27, 246, 172, 68, 183,
        155, 94, 220, 42, 35, 134, 139, 61, 96, 115, 165, 144, 153, 44, 15, 41, 117, 36, 61, 86,
        62, 161, 128, 210, 24, 238, 117, 124, 154,
    ];

    let len = determine_padding_v2_length(buffer.len());
    buffer.extend_from_slice(&random_padding[..len]);
    let msg_key = encrypt_data_v2(&mut buffer, &auth_key);

    assert_eq!(&buffer[..], &expected[24..]);
    assert_eq!(msg_key, &expected[8..24]);
    assert_eq!(auth_key.key_id(), &expected[..8]);
}

#[test]
fn decrypt_server_data_v2() {
    let mut ciphertext = vec![
        122, 113, 131, 194, 193, 14, 79, 77, 249, 69, 250, 154, 154, 189, 53, 231, 195, 132, 11,
        97, 240, 69, 48, 79, 57, 103, 76, 25, 192, 226, 9, 120, 79, 80, 246, 34, 106, 7, 53, 41,
        214, 117, 201, 44, 191, 11, 250, 140, 153, 167, 155, 63, 57, 199, 42, 93, 154, 2, 109, 67,
        26, 183, 64, 124, 160, 78, 204, 85, 24, 125, 108, 69, 241, 120, 113, 82, 78, 221, 144, 206,
        160, 46, 215, 40, 225, 77, 124, 177, 138, 234, 42, 99, 97, 88, 240, 148, 89, 169, 67, 119,
        16, 216, 148, 199, 159, 54, 140, 78, 129, 100, 183, 100, 126, 169, 134, 18, 174, 254, 148,
        44, 93, 146, 18, 26, 203, 141, 176, 45, 204, 206, 182, 109, 15, 135, 32, 172, 18, 160, 109,
        176, 88, 43, 253, 149, 91, 227, 79, 54, 81, 24, 227, 186, 184, 205, 8, 12, 230, 180, 91,
        40, 234, 197, 109, 205, 42, 41, 55, 78,
    ];
    let auth_key = AuthKey::from_bytes([
        93, 46, 125, 101, 244, 158, 194, 139, 208, 41, 168, 135, 97, 234, 39, 184, 164, 199, 159,
        18, 34, 101, 37, 68, 62, 125, 124, 89, 110, 243, 48, 53, 48, 219, 33, 7, 232, 154, 169,
        151, 199, 160, 22, 74, 182, 148, 24, 122, 222, 255, 21, 107, 214, 239, 113, 24, 161, 150,
        35, 71, 117, 60, 14, 126, 137, 160, 53, 75, 142, 195, 100, 249, 153, 126, 113, 188, 105,
        35, 251, 134, 232, 228, 52, 145, 224, 16, 96, 106, 108, 232, 69, 226, 250, 1, 148, 9, 119,
        239, 10, 163, 42, 223, 90, 151, 219, 246, 212, 40, 236, 4, 52, 215, 23, 162, 211, 173, 25,
        98, 44, 192, 88, 135, 100, 33, 19, 199, 150, 95, 251, 134, 42, 62, 60, 203, 10, 185, 90,
        221, 218, 87, 248, 146, 69, 219, 215, 107, 73, 35, 72, 248, 233, 75, 213, 167, 192, 224,
        184, 72, 8, 82, 60, 253, 30, 168, 11, 50, 254, 154, 209, 152, 188, 46, 16, 63, 206, 183,
        213, 36, 146, 236, 192, 39, 58, 40, 103, 75, 201, 35, 238, 229, 146, 101, 171, 23, 160, 2,
        223, 31, 74, 162, 197, 155, 129, 154, 94, 94, 29, 16, 94, 193, 23, 51, 111, 92, 118, 198,
        177, 135, 3, 125, 75, 66, 112, 206, 233, 204, 33, 7, 29, 151, 233, 188, 162, 32, 198, 215,
        176, 27, 153, 140, 242, 229, 205, 185, 165, 14, 205, 161, 133, 42, 54, 230, 53, 105, 12,
        142,
    ]);
    let expected = vec![
        252, 130, 106, 2, 36, 139, 40, 253, 96, 242, 196, 130, 36, 67, 173, 104, 1, 240, 193, 194,
        145, 139, 48, 94, 2, 0, 0, 0, 88, 0, 0, 0, 220, 248, 241, 115, 2, 0, 0, 0, 1, 168, 193,
        194, 145, 139, 48, 94, 1, 0, 0, 0, 28, 0, 0, 0, 8, 9, 194, 158, 196, 253, 51, 173, 145,
        139, 48, 94, 24, 168, 142, 166, 7, 238, 88, 22, 252, 130, 106, 2, 36, 139, 40, 253, 1, 204,
        193, 194, 145, 139, 48, 94, 2, 0, 0, 0, 20, 0, 0, 0, 197, 115, 119, 52, 196, 253, 51, 173,
        145, 139, 48, 94, 100, 8, 48, 0, 0, 0, 0, 0, 252, 230, 103, 4, 163, 205, 142, 233, 208,
        174, 111, 171, 103, 44, 96, 192, 74, 63, 31, 212, 73, 14, 81, 246,
    ];

    let plaintext = decrypt_data_v2(&mut ciphertext, &auth_key).unwrap();

    assert_eq!(plaintext, expected);
}

#[test]
fn key_from_nonce() {
    let server_nonce = {
        let mut buffer = [0u8; 16];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(i, x)| *x = i as u8);
        buffer
    };
    let new_nonce = {
        let mut buffer = [0u8; 32];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(i, x)| *x = i as u8);
        buffer
    };

    let (key, iv) = generate_key_data_from_nonce(&server_nonce, &new_nonce);
    assert_eq!(
        key,
        [
            7, 88, 241, 83, 59, 97, 93, 36, 246, 232, 169, 74, 111, 203, 238, 10, 85, 234, 171, 34,
            23, 215, 41, 92, 169, 33, 61, 26, 45, 125, 22, 166
        ]
    );
    assert_eq!(
        iv,
        [
            90, 132, 16, 142, 152, 5, 101, 108, 232, 100, 7, 14, 22, 110, 98, 24, 246, 120, 62,
            133, 17, 71, 26, 90, 183, 128, 44, 242, 0, 1, 2, 3
        ]
    );
}

#[test]
fn verify_ige_encryption() {
    let mut plaintext = get_test_aes_key_or_iv(); // Encrypting the key with itself
    let key = get_test_aes_key_or_iv();
    let iv = get_test_aes_key_or_iv();
    let expected = vec![
        226, 129, 18, 165, 62, 92, 137, 199, 177, 234, 128, 113, 193, 51, 105, 159, 212, 232, 107,
        38, 196, 186, 201, 252, 90, 241, 171, 140, 226, 122, 68, 164,
    ];

    aes::ige_encrypt(&mut plaintext, &key, &iv);
    let ciphertext = plaintext;

    assert_eq!(ciphertext.as_slice(), &expected);
}

#[test]
fn verify_ige_decryption() {
    let mut ciphertext = get_test_aes_key_or_iv(); // Decrypting the key with itself
    let key = get_test_aes_key_or_iv();
    let iv = get_test_aes_key_or_iv();
    let expected = vec![
        229, 119, 122, 250, 205, 123, 44, 22, 247, 172, 64, 202, 230, 30, 246, 3, 254, 230, 9, 143,
        184, 168, 134, 10, 185, 238, 103, 44, 215, 229, 186, 204,
    ];

    aes::ige_decrypt(&mut ciphertext, &key, &iv);
    let plaintext = ciphertext;

    assert_eq!(plaintext.as_slice(), &expected);
}

#[test]
fn since_epoch_is_after_2024() {
    // 2024-01-01; the clock of a machine running the tests is not set before it
    assert!(since_epoch().as_secs() > 1_704_067_200);
}
