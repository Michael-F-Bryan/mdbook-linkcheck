/// This module provides an (experimental ad-hoc) functionality of
/// supporting latex in `mdbook-linkcheck`.
use std::collections::HashSet;

/// A struct that maps text changes from file B to file A, where file
/// A is original and B is modified. It is used to map back error
/// positions after A is altered into B by regexes that cut out latex
/// fragments.
pub(crate) struct ByteIndexMap {
    /// Mapping from B to A stored as (b_i,a_i), stored as
    /// monotonously increased pairs.
    ///
    /// I.e. it always holds that b_{i+1} > b_{i} && a_{i+1} > a_i.
    mapping: Vec<(u32, u32)>,
    /// Ranges in a that are altered.
    inserted_ranges_a: HashSet<u32>,
}

impl ByteIndexMap {
    pub fn new() -> Self {
        ByteIndexMap {
            mapping: vec![],
            inserted_ranges_a: HashSet::new(),
        }
    }

    // Internal contsistency check function. It can be turned off for
    // efficiency if latex support becomes too slow. But for now I prefer to
    // leave it here @volhovm.
    fn consistency_check(&self, s: &str) {
        let mut prev_b: u32 = 0;
        let mut prev_a: u32 = 0;
        for (ix, (b, a)) in self.mapping.iter().enumerate() {
            if b < &prev_b || a < &prev_a {
                panic!(
                    "Inconsistent {}, ix {:?}, value {:?}, prev values {:?}",
                    s,
                    ix,
                    (b, a),
                    (prev_b, prev_a)
                );
            }
            prev_b = *b;
            prev_a = *a;
        }
    }

    pub fn update(&mut self, start: u32, end: u32, len_b: u32) {
        assert!(end >= start);
        let start_end_range: Vec<u32> = (start..end).collect();
        for i in start_end_range.iter() {
            assert!(
                !self.inserted_ranges_a.contains(i),
                "Collision on {:?}",
                i
            );
            self.inserted_ranges_a.insert(*i);
        }
        self.consistency_check("Before update");
        let insert_ix = match self
            .mapping
            .iter()
            .enumerate()
            .find(|(_ix, (_pos_b, pos_a))| pos_a > &start)
        {
            Some((ix, (_, pos_a))) => {
                // chunks must not overlap
                assert!(end < *pos_a);
                ix
            },
            None => self.mapping.len(),
        };
        let (pos_b, pos_a) = if insert_ix > 0 {
            self.mapping[insert_ix - 1]
        } else {
            (0, 0)
        };
        assert!(start >= pos_a);
        let delta_same = start - pos_a;
        // A: (start,end)
        // ... maps to
        // B: (cur_b + delta_same, cur_b + delta_same + repl_length)
        let new_a = end;
        let new_b = pos_b + (delta_same + len_b);
        assert!(new_a >= pos_a);
        assert!(new_b >= pos_b);
        self.mapping.insert(insert_ix, (new_b, new_a));

        // Remap all the following pieces.
        let mut prev_b: u32 = new_b;
        let len_a = end - start;
        for i in insert_ix + 1..self.mapping.len() {
            let (b, a) = self.mapping[i];
            let updated_b = b - len_a + len_b;
            self.mapping[i] = (updated_b, a);
            assert!(updated_b >= prev_b);
            prev_b = updated_b;
        }
        self.consistency_check("After update");
    }

    /// Given a position in file B, returns a corresponding position in file A.
    pub fn resolve(&self, input_b: u32) -> u32 {
        let ix = match self
            .mapping
            .iter()
            .enumerate()
            .find(|(_ix, (pos_b, _pos_a))| pos_b > &input_b)
        {
            Some((ix, _)) => ix,
            None => self.mapping.len(),
        };
        let (pos_b, pos_a) = if ix > 0 { self.mapping[ix - 1] } else { (0, 0) };

        pos_a + (input_b - pos_b)
    }
}

/// Filters out latex code snippets from md files to avoid false link
/// matches.
pub(crate) fn filter_out_latex(src: &str) -> (String, ByteIndexMap) {
    use regex::Regex;

    let mut byte_index_map = ByteIndexMap::new();
    let mut src: String = src.to_string();

    //println!("\n\n\nFile: {}", src);

    let mut process_regex = |regex_expr: &str, replacement: &str| {
        let mut byte_index_map_upds = vec![];
        let reg = Regex::new(regex_expr).unwrap();
        for captures in reg.captures_iter(&src) {
            if let Some(mtch) = captures.get(0) {
                let start = mtch.start() as u32;
                let end = mtch.end() as u32;

                let repl_length = replacement.len() as u32;
                byte_index_map_upds.push((
                    byte_index_map.resolve(start),
                    byte_index_map.resolve(start) + end - start,
                    repl_length,
                ));
            }
        }

        // update source and byte_index_map
        for (start, end, length) in byte_index_map_upds {
            byte_index_map.update(start, end, length);
        }
        src = reg.replace_all(&src, replacement).to_string();
    };

    // Everything between a pair of $$ including newlines
    process_regex(r"\$\$[^\$]*\$\$", "LATEX_DOUBLE_DOLLAR_SUBSTITUTED");
    // Everything between a pair of $ excluding newlines
    process_regex(r"\$[^\$\n\r]*\$", "LATEX_SINGLE_DOLLAR_SUBSTITUTED");
    // Everything between \( and \) excluding newlines
    process_regex(r"\\\([^\n\r]*\\\)", "LATEX_ESCAPED_PARENTHESIS_SUBSTITUTED");
    // Everything between \[ and \] including newlines
    process_regex(
        r"\\\[(.|\r\n|\r|\n)*\\\]",
        "LATEX_ESCAPED_SQUARE_BRACKET_SUBSTITUTED",
    );

    //println!("\n\n\nFile after: {}", src);

    (src.to_string(), byte_index_map)
}
