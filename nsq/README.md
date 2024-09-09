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

## TODO

More documentation...
