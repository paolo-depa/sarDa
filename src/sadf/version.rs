//! The sysstat version of an SA file or of `sadf`.

use std::path::Path;

/// A sysstat version, ordered so that incompatibility can be tested with `>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl Version {
    /// Finds the first `N.N[.N]` in a string such as `sysstat version 12.7.9`.
    pub fn parse(text: &str) -> Option<Version> {
        for token in text.split_whitespace() {
            let token = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
            let mut parts = token.split('.');
            let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(major), Ok(minor)) = (major.parse(), minor.parse()) else {
                continue;
            };
            let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            return Some(Version {
                major,
                minor,
                patch,
            });
        }
        None
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// The file's version is read here, on the host, instead of asked from `sadf`:
// `sadf` returns no data at all for a file written by a newer sysstat, and it
// reports no version for a file it cannot parse, so only the file's own header
// can tell the two cases from an empty one (ADR 0007). The constants follow
// sysstat's `sa.h`; `fixture_versions_match_the_recorded_sadf` pins them to a
// real file.
pub(super) const SYSSTAT_MAGIC: u16 = 0xd596;
pub(super) const FORMAT_MAGIC: u16 = 0x2175;

/// What a file's magic header says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaHeader {
    /// A system activity file in the layout known here, written by this
    /// sysstat version.
    Current(Version),
    /// A system activity file in an older layout; `sadf -c` can convert it.
    OldFormat,
    /// Not a system activity file (a text report, a log, an empty file ...).
    NotSa,
}

/// Reads the magic header of `path`.
pub fn read_sa_header(path: &Path) -> SaHeader {
    use std::io::Read;
    let mut buf = [0u8; 8];
    if std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_err()
    {
        return SaHeader::NotSa;
    }
    let little = u16::from_le_bytes([buf[0], buf[1]]) == SYSSTAT_MAGIC;
    let big = u16::from_be_bytes([buf[0], buf[1]]) == SYSSTAT_MAGIC;
    if !little && !big {
        return SaHeader::NotSa;
    }
    let pair = [buf[2], buf[3]];
    let format = if little {
        u16::from_le_bytes(pair)
    } else {
        u16::from_be_bytes(pair)
    };
    if format == FORMAT_MAGIC {
        SaHeader::Current(Version {
            major: buf[4],
            minor: buf[5],
            patch: buf[6],
        })
    } else {
        SaHeader::OldFormat
    }
}

/// The sysstat version a system activity file was written with. Errors when
/// the file is not an SA file, or when it is in a format older than the one
/// whose layout is known here.
pub fn read_sa_version(path: &Path) -> std::result::Result<Version, String> {
    match read_sa_header(path) {
        SaHeader::Current(version) => Ok(version),
        SaHeader::OldFormat => Err("old system activity file format".into()),
        SaHeader::NotSa => Err("not a system activity file".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Scratch;

    #[test]
    fn version_parsing_and_ordering() {
        let v = Version::parse("sysstat version 12.7.9").expect("version");
        assert_eq!((v.major, v.minor, v.patch), (12, 7, 9));
        assert_eq!(
            Version::parse("12.7").unwrap(),
            Version {
                major: 12,
                minor: 7,
                patch: 0
            }
        );
        assert!(Version::parse("(C) Sebastien Godard").is_none());
        assert!(
            v < Version {
                major: 12,
                minor: 8,
                patch: 0
            }
        );
        assert!(
            v > Version {
                major: 12,
                minor: 7,
                patch: 5
            }
        );
        assert!(
            v > Version {
                major: 11,
                minor: 9,
                patch: 9
            }
        );
    }

    #[test]
    fn reads_the_version_from_the_file_header() {
        let dir = Scratch::new("sadf-header");
        let header = |magic: fn(u16) -> [u8; 2], version: [u8; 3]| {
            let mut b = Vec::new();
            b.extend_from_slice(&magic(0xd596));
            b.extend_from_slice(&magic(0x2175));
            b.extend_from_slice(&version);
            b.extend_from_slice(&[0, 0, 0, 0]);
            b
        };
        let le = dir.join("le");
        std::fs::write(&le, header(u16::to_le_bytes, [12, 8, 1])).unwrap();
        assert_eq!(
            read_sa_version(&le).unwrap(),
            Version {
                major: 12,
                minor: 8,
                patch: 1
            }
        );

        let be = dir.join("be");
        std::fs::write(&be, header(u16::to_be_bytes, [12, 7, 9])).unwrap();
        assert_eq!(
            read_sa_version(&be).unwrap(),
            Version {
                major: 12,
                minor: 7,
                patch: 9
            }
        );

        let junk = dir.join("junk");
        std::fs::write(&junk, b"not an sa file at all").unwrap();
        assert!(read_sa_version(&junk).is_err());
        let short = dir.join("short");
        std::fs::write(&short, b"abc").unwrap();
        assert!(read_sa_version(&short).is_err());
        assert_eq!(read_sa_header(&junk), SaHeader::NotSa);
        assert_eq!(read_sa_header(&short), SaHeader::NotSa);
        assert_eq!(read_sa_header(&dir.join("absent")), SaHeader::NotSa);

        let old = dir.join("old");
        let mut bytes = SYSSTAT_MAGIC.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0x2171u16.to_le_bytes());
        bytes.extend_from_slice(&[9, 1, 6, 0, 0, 0]);
        std::fs::write(&old, bytes).unwrap();
        assert_eq!(read_sa_header(&old), SaHeader::OldFormat);
    }

    #[test]
    fn fixture_versions_match_the_recorded_sadf() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let stream = std::fs::read_to_string(root.join("sadf.stream")).unwrap();
        let recorded = stream
            .lines()
            .find_map(|l| l.strip_prefix("@@VERSION "))
            .and_then(Version::parse)
            .expect("recorded @@VERSION");
        assert_eq!(read_sa_version(&root.join("sa/sa20261125")), Ok(recorded));
    }
}
