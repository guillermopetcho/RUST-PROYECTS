#!/usr/bin/env bash
set -e

# Directorio del proyecto
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

BIN="$DIR/target/release/mopet"

# Compilar en modo release si no existe el binario
if [ ! -f "$BIN" ]; then
    echo "Compilando MOPET en modo release optimizado..."
    cargo build --release --manifest-path "$DIR/Cargo.toml"
fi

# Ejecutar pasando todos los argumentos
exec "$BIN" "$@"

