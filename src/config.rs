use regex::Regex;

/// The configuration options available with this backend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    /// If a link on the internet is encountered, should we still try to check
    /// if it's valid? Defaults to `false` because this has a big performance
    /// impact.
    pub follow_web_links: bool,
    /// Are we allowed to link to files outside of the book's source directory?
    pub traverse_parent_directories: bool,
    #[serde(with = "regex_serde")]
    pub exclude: Vec<Regex>,
}

impl Config {
    pub fn should_skip(&self, link: &str) -> bool {
        self.exclude.iter().any(|pat| pat.is_match(link))
    }
}

mod regex_serde {
    use regex::Regex;
    use serde::de::{Deserialize, Deserializer, Error};
    use serde::ser::{SerializeSeq, Serializer};

    pub fn serialize<S>(re: &Vec<Regex>, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = ser.serialize_seq(Some(re.len()))?;

        for pattern in re {
            seq.serialize_element(pattern.as_str())?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(de: D) -> Result<Vec<Regex>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Vec::<String>::deserialize(de)?;
        let mut patterns = Vec::new();

        for pat in raw {
            let re = Regex::new(&pat).map_err(D::Error::custom)?;
            patterns.push(re);
        }

        Ok(patterns)
    }
}

impl PartialEq for Config {
    fn eq(&self, other: &Config) -> bool {
        let Config {
            follow_web_links,
            traverse_parent_directories,
            ref exclude,
        } = self;

        *follow_web_links == other.follow_web_links
            && *traverse_parent_directories == other.traverse_parent_directories
            && exclude.len() == other.exclude.len()
            && exclude
                .iter()
                .zip(other.exclude.iter())
                .all(|(l, r)| l.as_str() == r.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml;
    const CONFIG: &str = r#"follow-web-links = true
traverse-parent-directories = true
exclude = ["google\\.com"]
"#;

    #[test]
    fn deserialize_a_config() {
        let should_be = Config {
            follow_web_links: true,
            traverse_parent_directories: true,
            exclude: vec![Regex::new(r"google\.com").unwrap()],
        };

        let got: Config = toml::from_str(CONFIG).unwrap();

        assert_eq!(got, should_be);
    }

    #[test]
    fn round_trip_config() {
        let deserialized: Config = toml::from_str(CONFIG).unwrap();
        let reserialized = toml::to_string(&deserialized).unwrap();

        assert_eq!(reserialized, CONFIG);
    }
}
