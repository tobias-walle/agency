pub(crate) const MAX_BUFFER_BYTES: usize = 1024 * 1024; // ~1 MiB cap for history ring
pub(crate) const ATTACH_REPLAY_BYTES: usize = 128 * 1024; // 128 KiB replay limit
pub(crate) const ATTACH_REPLAY_EMIT_BYTES: usize = 8 * 1024; // Emit up to 8 KiB on initial prefill
