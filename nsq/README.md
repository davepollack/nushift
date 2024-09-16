# nsq

A client/server network protocol for serving Nushift apps.

## Overview

It should be as easy to start a secure server serving Nushift apps as it is to start an SSH server.

This requirement is the reason for creating a new protocol.

A TOFU (trust-on-first-use) scheme is used. When a server is first started, a key is randomly generated. When a client first connects to a server, it accepts the key with no prompt to the user. On subsequent attempts, if the key has changed then a warning is shown.

## Protocol

[QUIC](https://www.rfc-editor.org/rfc/rfc9000.html) and [HTTP/3](https://www.rfc-editor.org/rfc/rfc9114.html) are used, with the underlying crypto in the QUIC CRYPTO frames being swapped out from TLS to [Noise](https://noiseprotocol.org/noise.html).

An ephemeral post-quantum exchange was added to the Noise handshake pattern through [Noise HFS](https://github.com/noiseprotocol/noise_hfs_spec/blob/025f0f60cb3b94ad75b68e3a4158b9aac234f8cb/output/noise_hfs.pdf).

XXhfs is the pattern being used at the moment.

Finally, a curve with higher security margins was chosen. Therefore, the final Noise protocol name is `Noise_XXhfs_448+Kyber1024_ChaChaPoly_BLAKE2b`.

## Handshake phase

Here is a diagram outlining the messages in the handshake phase:

```mermaid
sequenceDiagram
    rect rgb(191, 223, 255)
        Note over Client,Server: QUIC Initial packet space
        Client->>Server: Noise message length (2 bytes)<br/>Noise: e, e1, payload: QUIC Transport Parameters
    end

    rect rgb(231, 209, 255)
        Note over Client,Server: QUIC Handshake packet space
        Server->>Client: Noise message length (2 bytes)<br />Noise: e, ee, ekem1, s, es, payload: QUIC Transport Parameters
        Client->>Server: Noise message length (2 bytes)<br />Noise: s, se, payload: Empty
    end

    rect rgb(255, 209, 184)
        Note over Client,Server: QUIC Application data packet space
        Client<<-->>Server: Transport phase traffic
    end
```

In the messages, the Noise message length is included because the post-quantum keys and ciphertexts make the messages so huge (1700-2000 bytes per post-quantum message) as to not fit in a single UDP datagram anymore. And QUIC does not give us the ability to otherwise signify the end of a message within a stream beyond the stream abstraction it does give us.

### Initial secrets

The secrets used for the QUIC packet protection layer in the QUIC Initial packet space are derived as in [RFC 9001](https://www.rfc-editor.org/rfc/rfc9001.html#initial-secrets), except, instead of [HKDF-Expand-Label from TLS 1.3](https://www.rfc-editor.org/rfc/rfc8446.html#section-7.1) being used, only HKDF-Expand is used, with the strings "client in"/"server in" simply forming the `info` for HKDF-Expand rather than the `label` for HKDF-Expand-Label. The reason being, it is not relevant for us to use the string "tls13" which is in the definition of HKDF-Expand-Label, and also avoids other complexity that HKDF-Expand-Label introduces, namely the encoding of multiple fields and length encodings of those fields, of a struct into `info`.

The QUIC Transport Parameters in this first message are effectively unencrypted as is the case in TLS 1.3.

### Handshake secrets

Ideally, I don't want to use the Handshake packet space, and just rely on Noise's protection. However, it is needed for `quinn` to work, and specifically, even the second message needs to be in this space for `quinn` to work (not only the last message).

So, we derive "secrets" after the first message from calling `Split()` on the Noise state. This is in contrast to TLS 1.3, where Handshake secrets are derived after the DH exchange and thus do provide protection. I.e., in TLS 1.3 the second UDP datagram contains an Initial QUIC packet with just the server ephemeral key and a Handshake QUIC packet with the remainder of the data which is protected. We cannot do this approach because the `snow` Noise library does not allow partially processing a Noise message.

Like TLS 1.3, the server QUIC Transport Parameters here are protected, due to Noise's protection.

## Transport phase

As Noise specifies, the secrets for the transport phase are derived by calling `Split()` on the Noise state after all handshake messages have been processed. The QUIC packet protection layer, using ChaCha20-Poly1305, then performs the function of encrypting and decrypting transport messages in a similar way to the Noise specification for transport messages. However, the exact parameters used for the encryption and decryption operations are different, and are detailed below.

## Packet protection

The QUIC packet protection layer applies at all of the Initial, Handshake and Application data (transport phase) stages. We use ChaCha20-Poly1305 for this layer, with a key, nonce and associated data as below.

The key is derived further from the secret that was derived as described in the earlier sections. For the same reasons described in the Initial secrets section, the key for packet protection is also derived using only HKDF-Expand, not TLS 1.3's HKDF-Expand-Label. The string "quic key" is provided directly as the `info` for HKDF-Expand, and the input PRK is the secret derived as described earlier either for the Initial, Handshake or Application data stages.

The nonce is the QUIC packet number, like how an explicit nonce is suggested by the Noise specification for out-of-order protocols. Specifically, per the Noise specification for ChaChaPoly cipher functions, the 96-bit nonce is formed from 32 bits of zeros followed by a little-endian encoding of the 64-bit nonce (QUIC packet number). This is unlike TLS 1.3, where a key-derived IV (using the label "quic iv") is also mixed into the nonce, the purpose of which is to defend against multitarget attacks. Noise, WireGuard and other Noise-over-QUIC proposals do not do this. Presumably, this is because Noise forces a key size of 256 bits, which has previously been suggested as an alternative to defend against multitarget attacks rather than using a session-specific IV with a 128-bit key.

The associated data is the QUIC unprotected header, as in RFC 9001.

## Header protection

Header protection is applied in almost the same way as RFC 9001, using its ChaCha20-based header protection and the raw ChaCha20 function, except, the header protection key `hp_key` is derived from the encryption-level secret using only HKDF-Expand, not TLS 1.3's HKDF-Expand-Label, for the same reasons described in the Initial secrets section, with the string "quic hp" provided directly as the `info` for HKDF-Expand.
