//! Parser for `docker://` URLs.
//!
//! This module provides support for parsing image references.
//!
//! ## Example
//!
//! ```rust
//! # extern crate dkregistry;
//! # fn main() {
//! # fn run() -> dkregistry::errors::Result<()> {
//! #
//! use std::str::FromStr;
//! use dkregistry::reference::Reference;
//!
//! // Parse an image reference
//! let dkref = Reference::from_str("docker://busybox")?;
//! assert_eq!(dkref.registry(), "registry-1.docker.io");
//! assert_eq!(dkref.repository(), "library/busybox");
//! assert_eq!(dkref.version(), "latest");
//! #
//! # Ok(())
//! # };
//! # run().unwrap();
//! # }
//! ```
//!
//!

// The `docker://` schema is not officially documented, but has a reference implementation:
// https://github.com/docker/distribution/blob/v2.6.1/reference/reference.go

use errors::Error;
use regex;
use std::collections::VecDeque;
use std::str::FromStr;
use std::{fmt, str};

static DEFAULT_REGISTRY: &str = "registry-1.docker.io";
static DEFAULT_TAG: &str = "latest";
static DEFAULT_SCHEME: &str = "docker";

/// Image version, either a tag or a digest.
#[derive(Clone)]
pub enum Version {
    Tag(String),
    Digest(String, String),
}

impl str::FromStr for Version {
    type Err = ::errors::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = match s.chars().nth(0) {
            Some(':') => Version::Tag(s.trim_left_matches(':').to_string()),
            Some('@') => {
                let r: Vec<&str> = s.trim_left_matches('@').splitn(2, ':').collect();
                if r.len() != 2 {
                    bail!("wrong digest format");
                };
                Version::Digest(r[0].to_string(), r[1].to_string())
            }
            Some(_) => bail!("unknown prefix"),
            None => bail!("too short"),
        };
        Ok(v)
    }
}

impl Default for Version {
    fn default() -> Self {
        Version::Tag("latest".to_string())
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let v = match *self {
            Version::Tag(ref s) => ":".to_string() + s,
            Version::Digest(ref t, ref d) => "@".to_string() + t + ":" + d,
        };
        write!(f, "{}", v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let v = match *self {
            Version::Tag(ref s) => s.to_string(),
            Version::Digest(ref t, ref d) => t.to_string() + ":" + d,
        };
        write!(f, "{}", v)
    }
}

/// A registry image reference.
#[derive(Clone, Debug, Default)]
pub struct Reference {
    has_schema: bool,
    raw_input: String,
    registry: String,
    repository: String,
    version: Version,
}

impl Reference {
    pub fn new(registry: Option<String>, repository: String, version: Option<Version>) -> Self {
        let reg = registry.unwrap_or_else(|| DEFAULT_REGISTRY.to_string());
        let ver = version.unwrap_or_else(|| Version::Tag(DEFAULT_TAG.to_string()));
        Self {
            has_schema: false,
            raw_input: "".into(),
            registry: reg,
            repository,
            version: ver,
        }
    }

    pub fn registry(&self) -> String {
        self.registry.clone()
    }

    pub fn repository(&self) -> String {
        self.repository.clone()
    }

    pub fn version(&self) -> String {
        self.version.to_string()
    }

    pub fn to_raw_string(&self) -> String {
        self.raw_input.clone()
    }

    //TODO(lucab): move this to a real URL type
    pub fn to_url(&self) -> String {
        format!(
            "{}://{}/{}{:?}",
            DEFAULT_SCHEME, self.registry, self.repository, self.version
        )
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}/{}{:?}", self.registry, self.repository, self.version)
    }
}

impl str::FromStr for Reference {
    type Err = ::errors::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_url(s)
    }
}

fn parse_url(input: &str) -> Result<Reference, Error> {
    // TODO(lucab): investigate using a grammar-based parser.
    let mut rest = input;

    // Detect and remove schema.
    let has_schema = rest.starts_with("docker://");
    if has_schema {
        rest = input.trim_left_matches("docker://");
    };

    // Split path components apart and retain non-empty ones.
    let mut components: VecDeque<String> = rest
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    // Take image name and extract tag or digest-ref, if any.
    let last = components
        .pop_back()
        .ok_or(Error::from("missing image name"))?;
    let (image_name, version) = match (last.rfind('@'), last.rfind(':')) {
        (Some(i), _) | (None, Some(i)) => {
            let s = last.split_at(i);
            (String::from(s.0), Version::from_str(s.1)?)
        }
        (None, None) => (last, Version::default()),
    };
    ensure!(!image_name.is_empty(), "empty image name");

    // Handle images in default library namespace, that is:
    // `ubuntu` -> `library/ubuntu`
    if components.is_empty() {
        components.push_back("library".to_string());
    }
    components.push_back(image_name);

    // Take first component and check if it is a hostname or a path component,
    // according to regex at https://docs.docker.com/registry/spec/api/#overview.
    let first = components
        .pop_front()
        .ok_or(Error::from("missing image name"))?;
    let path_re = regex::Regex::new("^[a-z0-9]+(?:[._-][a-z0-9]+)*$")?;
    let registry = if path_re.is_match(&first) {
        components.push_front(first);
        DEFAULT_REGISTRY.to_string()
    } else {
        first
    };

    // Re-assemble repository name.
    let repository = components.into_iter().collect::<Vec<_>>().join("/");
    ensure!(!repository.is_empty(), "empty repository name");
    ensure!(repository.len() <= 127, "repository name too long");

    Ok(Reference {
        has_schema,
        raw_input: input.to_string(),
        registry,
        repository,
        version,
    })
}
