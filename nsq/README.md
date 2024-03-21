# nsq

A work-in-progress crate, attempting to implement PQNoise using the xwing-kem.rs crate as a KEM. The pqXX pattern is initially supported.

It may also attempt to integrate this into QUIC. As pqXX uses four messages, I don't yet know how possible this is.

Update: Currently implementing Noise HFS, not PQNoise or X-Wing, with the protocol string "Noise_XXhfs_448+Kyber1024_ChaChaPoly_BLAKE2b", as Snow mostly has support for this so is an easier route to implementation.
