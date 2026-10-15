/// OS-level adapters behind traits.
///
/// Platform-specific implementations live in cfg-gated sub-modules;
/// callers depend only on the trait interfaces exposed here.
pub mod volume;
