# Dependencies

## getrandom

Random private DH exponents, `random_id`, `random_bytes` and the padding of encrypted messages.

## md5

Fingerprint of an encrypted file's key and iv (`md5(key + iv)`), as the end-to-end docs require.

## mtg-crypto

AES-IGE, SHA-1/SHA-256 macros and the safe-prime check for the DH parameters.

## mtg-tl-types

The API types and functions of the secret chat methods (`messages.requestEncryption`, ...).

## num-bigint

Modular exponentiation of the Diffie-Hellman exchange.

## toml

Used to test that this file lists all dependencies from `Cargo.toml`.
