@echo off
REM One-shot build (use when code changed; avoids cargo lock while editor is open).
cargo build %*
