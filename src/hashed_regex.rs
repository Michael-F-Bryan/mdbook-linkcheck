use regex::Regex;
use serde::{Serialize, Deserialize, Deserializer, de::Error};
use std::{
    hash::{Hash, Hasher},
    ops::Deref,
    str::FromStr
};

/// A wrapper around [`regex::Regex`] which implements **string repr based**
/// [`Serialize`], [`Deserialize`], [`PartialEq`], [`Eq`], [`Hash`].
///
/// It also implements `Deref<Target=Regex>` and [`FromStr`] for convenience.
///
/// # Important
///
/// **All the implementations are string based**. It means that the said
/// implementations simply delegate to the underlying implementations for `str`.
///
/// For example, while `[0-9]*` and `\d*` are the same regex, they will be considered
/// different. In particular, the following is true:
/// ```
/// use mdbook_linkcheck::HashedRegex;
///
/// assert_ne!(
///     HashedRegex::new("[0-9]*").unwrap(),
///     HashedRegex::new(r"\d*").unwrap()
/// );
/// ```
#[derive(Serialize, Debug, Clone)]
#[serde(transparent)]
pub struct HashedRegex {
    /// String representation.
    pub string: String,

    /// Compiled regexp.
    #[serde(skip_serializing)]
    pub re: Regex
}

impl HashedRegex {
    /// Create new [`HashedRegex`] instance.
    pub fn new(s: &str) -> Result<Self, regex::Error> {
        let string = s.to_string();
        let re = Regex::new(s)?;

        Ok(HashedRegex { string, re })
    }
}

impl<'de> Deserialize<'de> for HashedRegex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        let string = String::deserialize(deserializer)?;
        let re = Regex::new(&string).map_err(D::Error::custom)?;

        Ok(HashedRegex { string, re })
    }
}

impl Hash for HashedRegex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.string.hash(state);
    }
}

impl PartialEq for HashedRegex {
    fn eq(&self, other: &Self) -> bool {
        self.string == other.string
    }
}

impl Eq for HashedRegex {}

impl FromStr for HashedRegex {
    type Err = regex::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        HashedRegex::new(s)
    }
}

impl Deref for HashedRegex {
    type Target = regex::Regex;

    fn deref(&self) -> &regex::Regex {
        &self.re
    }
}
