use fuzzcheck::{
    Mutator,
    mutators::{
        grammar::{
            AST, alternation, concatenation, grammar_based_ast_mutator, regex,
            repetition,
        },
        map::MapMutator,
    },
};

pub type UsefulStringMutator = impl Mutator<String>;

pub fn useful_string_mutator() -> UsefulStringMutator {
    MapMutator::new(
        grammar(),
        // this is an awkward hack
        |s: &String| {
            let list: Vec<AST> =
                s.chars().map(|char| AST::Token(char)).collect();
            Some((s.clone(), AST::Sequence(list)))
        },
        |(string, _): &(String, fuzzcheck::mutators::grammar::AST)| {
            string.clone()
        },
        |value, _| (value.as_bytes().len() * 8) as f64,
    )
}

fn grammar() -> impl Mutator<(String, fuzzcheck::mutators::grammar::AST)> {
    let grammar = alternation([
        repetition(regex("[ -~]"), 20..=1000),
        concatenation([
            regex("Person Lastname"),
            repetition(regex("[0-9]"), 1..=1000),
        ]),
        concatenation([regex("person"), repetition(regex("[0-9]"), 1..=1000)]),
        regex(r#"user@example\.com"#),
        repetition(regex("[ -~]"), 1..=100),
        regex(r#"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,4}"#),
        // try to make non-ascii characters less frequent (they may sometimes
        // be necessary to test a full set of behaviours, but they are messy to
        // work with)
        // alternation([regex("[ -~]"), regex("(.*?)")]),
    ]);
    grammar_based_ast_mutator(grammar).with_string()
}
