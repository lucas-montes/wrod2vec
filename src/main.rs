use rand::{seq::SliceRandom, thread_rng};
use rand_distr::{Distribution, Normal};
use regex::Regex;
use serde_json::{json, Value};
use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
};

#[derive(Debug)]
struct CorpusValues {
    words_map: HashMap<String, usize>,
    vec: Vec<usize>,
}

impl CorpusValues {
    fn new() -> Self {
        Self {
            words_map: HashMap::new(),
            vec: Vec::new(),
        }
    }
    fn populate<'a>(mut self, clean_corpus: Cow<'a, str>) -> Self {
        let stop_words = stop_words::get(stop_words::LANGUAGE::English);
        let mut index = 0;
        for word in clean_corpus.split_whitespace() {
            if !stop_words.iter().any(|s| s.eq(word)) {
                if self.words_map.contains_key(word) {
                    let i = self.words_map.get(word).unwrap();
                    self.vec.push(*i);
                } else {
                    //TODO: create the initial embeddings here
                    self.words_map.insert(word.to_string(), index);
                    self.vec.push(index);
                    index += 1;
                };
            }
        }
        self
    }
}

struct CBOWParams {
    vocab_size: usize,
    embeddings_dimension: usize, // Test with size 25 to 1000. More corpus more dimension
    random_samples: usize,
    mean: f32,
    std_dev: f32,
    window_size: usize,
    target: usize,
    learning_rate: f32,
    epochs: usize,
}
impl CBOWParams {
    fn set_random_samples(mut self, random_samples: usize) -> Self {
        self.random_samples = random_samples;
        self
    }
    fn set_embeddings_dimension(mut self, embeddings_dimension: usize) -> Self {
        self.embeddings_dimension = embeddings_dimension;
        self
    }
    fn set_mean(mut self, mean: f32) -> Self {
        self.mean = mean;
        self
    }
    fn set_std_dev(mut self, std_dev: f32) -> Self {
        self.std_dev = std_dev;
        self
    }
    fn set_window_size(mut self, window_size: usize) -> Self {
        self.window_size = window_size * 2 + 1;
        self.target = window_size;
        self
    }
    fn set_learning_rate(mut self, learning_rate: f32) -> Self {
        self.learning_rate = learning_rate;
        self
    }
    fn set_epochs(mut self, epochs: usize) -> Self {
        self.epochs = epochs;
        self
    }
    fn default() -> Self {
        let window_size = 2;
        Self {
            vocab_size: 0,
            random_samples: 30,
            embeddings_dimension: 100,
            mean: 0.0,
            std_dev: 0.01,
            window_size: window_size * 2 + 1,
            target: window_size,
            learning_rate: 0.01,
            epochs: 100,
        }
    }
    fn new(vocab_size: usize) -> Self {
        let mut result = Self::default();
        result.vocab_size = vocab_size;
        result
    }

    fn create_matrices(&self) -> (Vec<f32>, Vec<f32>) {
        // set the embeddings_dimension from and type
        let normal = Normal::new(self.mean, self.std_dev).unwrap();
        let mut rng = thread_rng();
        let input_matrix: Vec<f32> = (0..self.vocab_size)
            .flat_map(|_| {
                (0..self.embeddings_dimension)
                    .map(|_| normal.sample(&mut rng))
                    .collect::<Vec<f32>>()
            })
            .collect();
        let output_matrix = vec![0.0; self.vocab_size * self.embeddings_dimension];

        (input_matrix, output_matrix)
    }

    fn get_random_indices(&self, target: &usize, corpus: &[usize]) -> Vec<usize> {
        let mut rng = thread_rng();
        corpus
            .choose_multiple(&mut rng, self.random_samples)
            .cloned()
            .filter(|x| x.eq(target))
            .collect()
    }

    fn generate_pairs(&self, corpus: &[usize]) -> Vec<(Vec<usize>, usize)> {
        //TODO: iterator
        corpus
            .windows(self.window_size)
            .map(|w| {
                let mut window = w.to_vec();
                let target = window.remove(self.target);
                (window, target)
            })
            .collect()
    }
}

fn parse_corpus(mut corpus: String) -> CorpusValues {
    let re = Regex::new(r"[[:punct:]]").unwrap();
    corpus.make_ascii_lowercase();
    let clean_corpus = re.replace_all(&corpus, "");
    CorpusValues::new().populate(clean_corpus)
}

fn get_context_embedding(
    context_indices: &[usize],
    embeddings_dimension: usize,
    embeddings: &[f32],
) -> Vec<f32> {
    //TODO: perform either sum or average, currently only the sum.
    // Sum
    (0..embeddings_dimension)
        .map(|position| {
            context_indices
                .iter()
                .map(|context_index| embeddings[position + *context_index * embeddings_dimension])
                .sum()
        })
        .collect()
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn train(
    pairs: &[(Vec<usize>, usize)],
    cbow_params: &CBOWParams,
    input_layer: &mut Vec<f32>,
    mut hidden_layer: Vec<f32>,
    corpus: &CorpusValues,
) {
    for _ in 0..cbow_params.epochs {
        for (context, target) in pairs {
            // pass the input layer to the hidden layer
            let neu1 =
                get_context_embedding(&context, cbow_params.embeddings_dimension, &input_layer);

            // negative sampling
            let target_l2 = target * cbow_params.embeddings_dimension;
            let f: f32 = neu1
                .iter()
                .enumerate()
                .map(|(i, v)| v * hidden_layer[i + target_l2])
                .sum();

            let g = (1.0 - sigmoid(f)) * cbow_params.learning_rate;

            let mut neu1e: Vec<f32> = (0..cbow_params.embeddings_dimension)
                .map(|c| g * hidden_layer[c + target_l2])
                .collect();
            (0..cbow_params.embeddings_dimension)
                .for_each(|c| hidden_layer[c + target_l2] += g * neu1[c]);

            for negative_target in cbow_params.get_random_indices(&target, &corpus.vec) {
                let l2 = negative_target * cbow_params.embeddings_dimension;
                let f: f32 = neu1
                    .iter()
                    .enumerate()
                    .map(|(i, v)| v * hidden_layer[i + l2])
                    .sum();

                let g = (0.0 - sigmoid(f)) * cbow_params.learning_rate;

                (0..cbow_params.embeddings_dimension)
                    .for_each(|c| neu1e[c] += g * hidden_layer[c + l2]);
                (0..cbow_params.embeddings_dimension)
                    .for_each(|c| hidden_layer[c + l2] += g * neu1[c]);
            }

            // backpropagation, pass the hidden layer to the input layer
            context.iter().for_each(|index| {
                (0..cbow_params.embeddings_dimension).for_each(|c| {
                    input_layer[c + index * cbow_params.embeddings_dimension] += neu1e[c]
                })
            });
        }
    }
}

fn save_changes(file_path: &str, values: Vec<Value>) {
    let mut file = open_or_create_file(file_path);
    file.set_len(0).unwrap();
    file.write_all(serde_json::to_string_pretty(&values).unwrap().as_bytes())
        .expect("something");
}

fn open_or_create_file(file_path: &str) -> File {
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(file_path)
    {
        Ok(value) => value,
        Err(e) => panic!("Problem creating the file: {:?}", e),
    }
}

fn generate_result(
    word: &str,
    index: &usize,
    embeddings: &[f32],
    embeddings_dimension: usize,
) -> Value {
    let embedding: Vec<f32> = (0..embeddings_dimension)
        .map(|position| embeddings[position + index * embeddings_dimension])
        .collect();
    json!({"word": word, "embedding":embedding })
}

fn main() {
    let raw_corpus = "Today we will be learning about the fundamentals of data science and statistics. Data Science and statistics are hot and growing fields with alternative names of machine learning, artificial intelligence, big data, etc. I'm really excited to talk to you about data science and statistics because data science and statistics have long been a passions of mine. I didn't used to be very good at data science and statistics but after studying data science and statistics for a long time, I got better and better at it until I became a data science and statistics expert. I'm really excited to talk to you about data science and statistics, thanks for listening to me talk about data science and statistics.".to_string();

    let corpus = parse_corpus(raw_corpus);
    let cbow_params = CBOWParams::new(corpus.words_map.len())
        .set_embeddings_dimension(100)
        .set_epochs(300)
        .set_learning_rate(0.01);
    let pairs = cbow_params.generate_pairs(&corpus.vec);
    let (mut input_layer, hidden_layer) = cbow_params.create_matrices();
    train(
        &pairs,
        &cbow_params,
        &mut input_layer,
        hidden_layer,
        &corpus,
    );

    let values = corpus
        .words_map
        .into_iter()
        .map(|(k, v)| generate_result(&k, &v, &input_layer, cbow_params.embeddings_dimension))
        .collect();

    save_changes("result.json", values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params() -> CBOWParams {
        CBOWParams::new(4)
    }

    #[test]
    fn test_parse_corpus() {
        let words_map: HashMap<String, usize> = HashMap::from([
            ("uno".into(), 0),
            ("dos".into(), 1),
            ("tres".into(), 2),
            ("cinco".into(), 3),
        ]);
        let corpus = "uno and dos tres uno cinco".to_string();
        let clean = parse_corpus(corpus);
        assert_eq!(clean.vec, vec![0, 1, 2, 0, 3]);
        assert_eq!(clean.words_map, words_map);
    }

    #[test]
    fn test_create_matrices() {
        let cbow_params = default_params();
        let (input_layer, hidden_layer) = cbow_params.create_matrices();
        assert_eq!(input_layer.len(), hidden_layer.len());
        assert_eq!(input_layer.len(), 12);
    }

    #[test]
    fn test_generate_pairs() {
        let cbow_params = default_params();
        let (context, target) = cbow_params
            .generate_pairs(&[0, 1, 2, 0, 3])
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(context, &[0, 1, 0, 3]);
        assert_eq!(target, 2);
    }

    #[test]
    fn test_get_context_embedding() {
        let embeddings_dimension = 4;
        let embeddings = vec![
            0.1, 0.1, 0.1, 0.1, 0.1, 0.2, 0.1, 0.1, 1.0, 1.0, 1.0, 1.0, 0.1, 0.1, 0.3, 0.1, 0.4,
            0.1, 0.1, 0.1,
        ];
        let context = [0, 1, 3, 4];
        let context_embedding = get_context_embedding(&context, embeddings_dimension, &embeddings);
        assert_eq!(context_embedding, vec![0.7000000000000001, 0.5, 0.6, 0.4]);
    }
}
