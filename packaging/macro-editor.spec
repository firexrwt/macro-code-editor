Name:           macro-editor
Version:        0.1.0
Release:        1%{?dist}
Summary:        Fast terminal text editor with integrated file tree

License:        MIT
URL:            https://github.com/firexrwt/macro-code-editor
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo

ExclusiveArch:  %{rust_arches}

%description
macro is a fast terminal text editor written in Rust.
Features an integrated file tree, syntax highlighting for all major
languages, mouse support, and line numbers.
Config file: ~/.config/macro/config.toml

%prep
%autosetup

%build
cargo build --release

%install
install -Dm755 target/release/macro %{buildroot}%{_bindir}/macro

%files
%license LICENSE
%{_bindir}/macro

%changelog
* Tue Mar 03 2026 firexrwt <opensource@firexrwt.com> - 0.1.0-1
- Initial release
