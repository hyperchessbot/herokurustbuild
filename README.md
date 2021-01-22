# herokurustbuild

Heroku Rust build with Heroku CLI installed.

# Buildpack

https://github.com/emk/heroku-buildpack-rust

## Synopsis

```bash
heroku create {appname} --buildpack emk/rust

# for existing app
# heroku buildpacks:set emk/rust

git push heroku
```
