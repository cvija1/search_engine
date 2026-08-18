#[derive(Debug)]
pub struct Lexer<'a> {
    content: &'a [char],
}

impl<'a> Lexer<'a> {
    pub fn new(content: &'a [char]) -> Self {
        Self { content }
    }

    fn trim_left(&mut self) {
        while self.content.len() > 0 && self.content[0].is_whitespace() {
            self.content = &self.content[1..];
        }
    }

    fn chop(&mut self, n: usize) -> &'a [char] {
        let token = &self.content[0..n];
        self.content = &self.content[n..];
        token
    }

    fn chop_while<P>(&mut self, predicate: P) -> &'a [char]
    where
        P: Fn(&char) -> bool,
    {
        let mut n = 0;
        while n < self.content.len() && predicate(&self.content[n]) {
            n += 1
        }
        self.chop(n)
    }

    pub fn next_token(&mut self) -> Option<String> {
        self.trim_left();
        if self.content.len() == 0 {
            return None;
        }

        if self.content[0].is_numeric() {
            return Some(
                self.chop_while(|c| c.is_numeric())
                    .into_iter()
                    .map(|ch| ch.to_ascii_lowercase())
                    .collect::<String>(),
            );
        }

        if self.content[0].is_alphabetic() {
            let mut term = self
                .chop_while(|c| c.is_alphanumeric())
                .into_iter()
                .map(|ch| ch.to_ascii_lowercase())
                .collect::<String>();
            let mut env = crate::snowball::SnowballEnv::create(&term);
            crate::snowball::algorithms::english_stemmer::stem(&mut env);
            term = env.get_current().to_string();

            return Some(term);
        }

        return Some(
            self.chop(1)
                .into_iter()
                .map(|ch| ch.to_ascii_lowercase())
                .collect::<String>(),
        );
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}
