use std::path::Path;

use lv_core::traits::Chunker;
use lv_core::types::Chunk;

pub struct OverlappingChunker {
    pub chunk_size: usize,
    pub overlap: usize,
}

impl OverlappingChunker {
    pub fn new(chunk_size: usize, overlap: usize) -> Self {
        Self {
            chunk_size,
            overlap,
        }
    }
}

impl Chunker for OverlappingChunker {
    fn chunk(&self, text: &str, _file_path: Option<&Path>) -> Vec<Chunk> {
        let words: Vec<(usize, usize)> = word_byte_ranges(text);
        if words.is_empty() {
            return Vec::new();
        }

        let step = self.chunk_size.saturating_sub(self.overlap).max(1);
        let num_chunks = (words.len().saturating_sub(1)) / step + 1;
        let mut chunks = Vec::with_capacity(num_chunks);

        let mut start_word = 0;
        while start_word < words.len() {
            let end_word = (start_word + self.chunk_size).min(words.len());
            let byte_start = words[start_word].0;
            let byte_end = words[end_word - 1].1;
            chunks.push(Chunk {
                text: text[byte_start..byte_end].to_owned(),
                start_offset: start_word,
                end_offset: end_word,
            });
            start_word += step;
            if end_word >= words.len() {
                break;
            }
        }
        chunks
    }
}

fn word_byte_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut in_word = false;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if in_word {
                ranges.push((start, i));
                in_word = false;
            }
        } else if !in_word {
            start = i;
            in_word = true;
        }
    }
    if in_word {
        ranges.push((start, text.len()));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        let chunker = OverlappingChunker::new(5, 2);
        let chunks = chunker.chunk("", None);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let chunker = OverlappingChunker::new(5, 2);
        let chunks = chunker.chunk("   \t\n  ", None);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_single_chunk() {
        let chunker = OverlappingChunker::new(10, 2);
        let text = "one two three";
        let chunks = chunker.chunk(text, None);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "one two three");
        assert_eq!(chunks[0].start_offset, 0);
        assert_eq!(chunks[0].end_offset, 3);
    }

    #[test]
    fn test_overlapping_chunks() {
        let chunker = OverlappingChunker::new(3, 1);
        let text = "a b c d e";
        let chunks = chunker.chunk(text, None);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].text, "a b c");
        assert_eq!(chunks[1].text, "c d e");
    }

    #[test]
    fn test_chunk_indices() {
        let chunker = OverlappingChunker::new(2, 0);
        let text = "a b c d";
        let chunks = chunker.chunk(text, None);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].start_offset, 0);
        assert_eq!(chunks[0].end_offset, 2);
        assert_eq!(chunks[1].start_offset, 2);
        assert_eq!(chunks[1].end_offset, 4);
    }
}
