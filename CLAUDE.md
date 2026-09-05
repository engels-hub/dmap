# Project rules

## Documents

Write all Markdown files in this repository in Simplified Technical English (ASD-STE100).

- Use short sentences. Keep a sentence to 20 words or less.
- Use the active voice and the present tense.
- Put one topic in each sentence. Put one instruction in each sentence.
- Use the articles "a" and "the".
- Use a word with one meaning only. Use the same word for the same thing.
- Do not use more than three nouns in a row.
- Do not use "-ing" verb forms. Technical names are permitted.
- Keep a paragraph to six sentences or less.
- Write a warning or a caution before the related instruction.

Code blocks, tables and technical names are exempt from the vocabulary rules.

## Tools

The repository holds the `caveman`, `ponytail` and `ms-rust` skills in `.claude/skills/`. Each skill directory has its MIT license. The files in `.claude/skills/` come from other projects. The Simplified Technical English rule does not apply to them.

- `caveman` makes the assistant replies short. Type `/caveman` to start it.
- `ponytail` makes the assistant write the smallest code that does the work. Type `/ponytail` to start it.
- `ms-rust` applies the Microsoft Pragmatic Rust Guidelines to each `.rs` file. The skill starts on its own before Rust work.

Upstream sources: [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman), [DietrichGebert/ponytail](https://github.com/DietrichGebert/ponytail) and [lx-industries/ms-rust-skill](https://gitlab.com/lx-industries/ms-rust-skill).
