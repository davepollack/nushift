# nsq

A client/server network protocol for serving Nushift apps.

## Overview

It should be as easy to start a secure server serving Nushift apps as it is to start an SSH server.

This requirement is the reason for creating a new protocol.

A TOFU (trust-on-first-use) scheme is used. When a server is first started, a key is randomly generated. When a client first connects to a server, it accepts the key with no prompt to the user. On subsequent attempts, if the key has changed then a warning is shown.

## Old (TODO remove)

A work-in-progress crate supporting Noise HFS (using Snow), and attempting to integrate it into QUIC.

The protocol name currently being used is "Noise_XXhfs_448+Kyber1024_ChaChaPoly_BLAKE2b". In the future, a combination of XXhfs and IKhfs may be used.
