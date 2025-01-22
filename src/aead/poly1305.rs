// Copyright 2015-2025 Brian Smith.
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY
// SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
// OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
// CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

// TODO: enforce maximum input length.

use super::{Tag, TAG_LEN};
use crate::cpu;
#[cfg(all(target_arch = "arm", target_endian = "little"))]
use crate::cpu::GetFeature as _;

mod ffi_arm_neon;
mod ffi_fallback;

/// A Poly1305 key.
pub(super) struct Key {
    key_and_nonce: [u8; KEY_LEN],
}

pub(super) const BLOCK_LEN: usize = 16;
pub(super) const KEY_LEN: usize = 2 * BLOCK_LEN;

impl Key {
    #[inline]
    pub(super) fn new(key_and_nonce: [u8; KEY_LEN]) -> Self {
        Self { key_and_nonce }
    }
}

// Keep in sync with `poly1305_state` in ring-core/poly1305.h.
//
// The C code, in particular the way the `poly1305_aligned_state` functions
// are used, is only correct when the state buffer is 64-byte aligned.
#[repr(C, align(64))]
struct poly1305_state([u8; OPAQUE_LEN]);
const OPAQUE_LEN: usize = 512;

pub(super) enum Context {
    #[cfg(all(target_arch = "arm", target_endian = "little"))]
    ArmNeon(ffi_arm_neon::State),
    Fallback(ffi_fallback::State),
}

impl Context {
    #[inline]
    pub(super) fn from_key(key: Key, cpu: cpu::Features) -> Self {
        #[cfg(all(target_arch = "arm", target_endian = "little"))]
        if let Some(cpu) = cpu.get_feature() {
            return ffi_arm_neon::State::new_context(key, cpu);
        }
        let _: cpu::Features = cpu;
        ffi_fallback::State::new_context(key)
    }

    #[inline(always)]
    pub fn update(&mut self, input: &[u8]) {
        match self {
            #[cfg(all(target_arch = "arm", target_endian = "little"))]
            Self::ArmNeon(state) => state.update(input),
            Self::Fallback(state) => state.update(input),
        }
    }

    pub(super) fn finish(self) -> Tag {
        match self {
            #[cfg(all(target_arch = "arm", target_endian = "little"))]
            Self::ArmNeon(state) => state.finish(),
            Self::Fallback(state) => state.finish(),
        }
    }
}

/// Implements the original, non-IETF padding semantics.
///
/// This is used by chacha20_poly1305_openssh and the standalone
/// poly1305 test vectors.
pub(super) fn sign(key: Key, input: &[u8], cpu_features: cpu::Features) -> Tag {
    let mut ctx = Context::from_key(key, cpu_features);
    ctx.update(input);
    ctx.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test;

    // Adapted from BoringSSL's crypto/poly1305/poly1305_test.cc.
    #[test]
    pub fn test_poly1305() {
        let cpu_features = cpu::features();
        test::run(test_file!("poly1305_test.txt"), |section, test_case| {
            assert_eq!(section, "");
            let key = test_case.consume_bytes("Key");
            let key: &[u8; KEY_LEN] = key.as_slice().try_into().unwrap();
            let input = test_case.consume_bytes("Input");
            let expected_mac = test_case.consume_bytes("MAC");
            let key = Key::new(*key);
            let Tag(actual_mac) = sign(key, &input, cpu_features);
            assert_eq!(expected_mac, actual_mac.as_ref());

            Ok(())
        })
    }
}
