# Noto Sans Regular

Unmodified hinted TrueType font from the Noto project, under SIL OFL 1.1:
the same file OpenAC bundles (`assets/fonts/NotoSans/` there).

Source: https://github.com/notofonts/noto-fonts/blob/ffebf8c1ee449e544955a7e813c54f9b73848eac/hinted/ttf/NotoSans/NotoSans-Regular.ttf

SHA-256: b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5

Full licence: `OFL.txt` beside this file. The font is compiled into the
`ac-ui` crate (`include_bytes!` in `crates/ac-ui/src/bundled_font.rs`), so it
ships inside the `openac` executable; a release ships `OFL.txt` with it.
