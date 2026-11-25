use std::fmt;

/// Failure categories, each mapped to a stable process exit code (see README).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Other,
    Usage,
    NoRuntime,
    MissingImage,
    NoData,
    GrafanaFailed,
    PortUnavailable,
}

impl Kind {
    pub fn exit_code(self) -> i32 {
        match self {
            Kind::Other => 1,
            Kind::Usage => 2,
            Kind::NoRuntime => 3,
            Kind::MissingImage => 4,
            Kind::NoData => 5,
            Kind::GrafanaFailed => 6,
            Kind::PortUnavailable => 7,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Other => "error",
            Kind::Usage => "usage",
            Kind::NoRuntime => "no_runtime",
            Kind::MissingImage => "missing_image",
            Kind::NoData => "no_data",
            Kind::GrafanaFailed => "grafana_failed",
            Kind::PortUnavailable => "port_unavailable",
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub kind: Kind,
    pub message: String,
}

impl Error {
    pub fn new(kind: Kind, message: impl Into<String>) -> Self {
        Error {
            kind,
            message: message.into(),
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Error::new(Kind::Other, message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::other(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
