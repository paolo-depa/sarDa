//! Running `sadf` and decoding its output.
//!
//! One container run converts every SA file for every Activity. The shell
//! script inside prints a marker-delimited stream on stdout:
//!
//! ```text
//! @@VERSION sysstat version 12.7.9
//! @@FILE 0
//! @@PROBE 0 0
//! @@HDR {"sysstat": {"hosts": [{"nodename": "h", "file-date": "2026-11-25"}]}}
//! @@PERR <a sadf stderr line>
//! @@BEGIN per_cpu
//! <sadf -d output>
//! @@ERR <a sadf stderr line>
//! @@END
//! ```
//!
//! `@@VERSION` comes once, first: the version of the `sadf` that ran. Per file
//! the script first probes the header with `sadf -j -H` (falling back to
//! `sadf -c` for datafiles in an old format), reports `@@CONVERTED` when the
//! fallback worked, and skips the file with `@@SKIP` when it cannot be read or
//! when its day lies outside the Observation window. The same stream format is
//! what the test fixtures contain.

mod container;
mod script;
mod stream;
mod verdict;
mod version;

pub use container::run_in_container;
pub use script::build_script;
pub use stream::{parse_stream, FileProbe, Input, RawActivity, SadfOutput};
pub use verdict::{Incompatibility, State, Verdict};
pub use version::{read_sa_header, read_sa_version, SaHeader, Version};
