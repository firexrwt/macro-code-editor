Name:           macro-editor
Version:        0.1.0
Release:        1%{?dist}
Summary:        Fast terminal text editor with integrated file tree

License:        MIT
URL:            https://github.com/firexrwt/macro-editor
Source0:        %{name}-%{version}.tar.gz
# Вендоринг Cargo зависимостей (нужен для Koji offline build)
# Создать: cargo vendor && tar czf vendor.tar.gz vendor/
Source1:        vendor.tar.gz

# Fedora Rust macros
BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  rust-packaging

# syntect с default-fancy использует pure-Rust regex — C зависимости не нужны
ExclusiveArch:  %{rust_arches}

%description
macro is a fast terminal text editor written in Rust.

Features:
  - Integrated file tree (full screen when no file is open, split view otherwise)
  - Syntax highlighting for all major languages via syntect
  - Mouse support for navigation and selection
  - Line numbers
  - Standard shortcuts: Ctrl+S (save), Ctrl+Q (close/quit),
    Ctrl+C/X (copy/cut), Ctrl+V (paste)
  - Config file: ~/.config/macro/config.toml

%prep
%autosetup

# Настройка офлайн вендоринга для Koji
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

tar xf %{SOURCE1}

%build
%{cargo_build}

%install
install -Dm755 target/release/macro %{buildroot}%{_bindir}/macro

# Bash completion (опционально, если добавим в будущем)
# install -Dm644 completions/macro.bash %{buildroot}%{_datadir}/bash-completion/completions/macro

%files
%license LICENSE
%{_bindir}/macro

%changelog
* Mon Mar 03 2026 firexrwt <user@example.com> - 0.1.0-1
- Initial release
