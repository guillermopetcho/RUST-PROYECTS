#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

# Asegurar cargo en el PATH para la compilación
if [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

echo "=================================================================="
echo "         INSTALADOR AUTOMÁTICO DE MOPET PARA LA TERMINAL          "
echo "=================================================================="
echo ""

# 1. Compilar binario en modo release optimizado
echo "🔨 [1/3] Compilando MOPET en modo release optimizado..."
cargo build --release --manifest-path "$DIR/Cargo.toml"

BIN="$DIR/target/release/mopet"

if [ ! -f "$BIN" ]; then
    echo "❌ Error: No se pudo generar el binario en $BIN"
    exit 1
fi

# 2. Instalar el comando
echo "📦 [2/3] Instalando el comando 'mopet'..."

INSTALLED_SYSTEM=false

# Si somos root o podemos usar sudo, instalar en /usr/local/bin (disponible para TODOS los usuarios del sistema)
if [ "$EUID" -eq 0 ]; then
    mkdir -p /usr/local/bin
    install -m 755 "$BIN" /usr/local/bin/mopet
    INSTALLED_SYSTEM=true
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    sudo mkdir -p /usr/local/bin
    sudo install -m 755 "$BIN" /usr/local/bin/mopet
    INSTALLED_SYSTEM=true
fi

# Instalación a nivel de usuario (siempre como respaldo o si no hay sudo)
mkdir -p "$HOME/.local/bin" "$HOME/.cargo/bin" 2>/dev/null || true
cp -f "$BIN" "$HOME/.cargo/bin/mopet" 2>/dev/null || true
cp -f "$BIN" "$HOME/.local/bin/mopet" 2>/dev/null || true
chmod +x "$HOME/.cargo/bin/mopet" 2>/dev/null || true
chmod +x "$HOME/.local/bin/mopet" 2>/dev/null || true

# 3. Configurar variables de entorno en shells del usuario
echo "⚙️  [3/3] Configurando variables de entorno y alias de terminal..."

CONFIG_LINE='export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"'
ALIAS_LINE="alias mopet='$HOME/.local/bin/mopet'"

# Configurar ~/.bashrc si existe
if [ -f "$HOME/.bashrc" ]; then
    if ! grep -q "mopet" "$HOME/.bashrc" 2>/dev/null; then
        echo "" >> "$HOME/.bashrc"
        echo "# MOPET audio player" >> "$HOME/.bashrc"
        echo "$CONFIG_LINE" >> "$HOME/.bashrc"
        echo "$ALIAS_LINE" >> "$HOME/.bashrc"
    fi
fi

# Configurar ~/.zshrc si existe
if [ -f "$HOME/.zshrc" ]; then
    if ! grep -q "mopet" "$HOME/.zshrc" 2>/dev/null; then
        echo "" >> "$HOME/.zshrc"
        echo "# MOPET audio player" >> "$HOME/.zshrc"
        echo "$CONFIG_LINE" >> "$HOME/.zshrc"
        echo "$ALIAS_LINE" >> "$HOME/.zshrc"
    fi
fi

# Configurar ~/.profile si existe
if [ -f "$HOME/.profile" ]; then
    if ! grep -q ".cargo/bin" "$HOME/.profile" 2>/dev/null; then
        echo "$CONFIG_LINE" >> "$HOME/.profile"
    fi
fi

echo ""
echo "=================================================================="
if [ "$INSTALLED_SYSTEM" = true ]; then
    echo "🎉 ¡MOPET se instaló en /usr/local/bin/mopet para TODOS LOS USUARIOS!"
else
    echo "🎉 ¡MOPET se instaló correctamente en ~/.local/bin/mopet!"
fi
echo "=================================================================="
echo ""
echo "⚠️  IMPORTANTE para terminales ya abiertas:"
echo "   Ejecuta: source ~/.bashrc   (o abre una nueva ventana de terminal)"
echo ""
echo "Luego solo escribe:"
echo "   mopet"
echo ""
