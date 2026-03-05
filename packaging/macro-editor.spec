Name:           macro-editor
Version:        0.2.1
Release:        1%{?dist}
Summary:        Fast terminal text editor with integrated file tree

License:        MIT
URL:            https://github.com/firexrwt/macro-code-editor
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo

ExclusiveArch:  %{rust_arches}

%description
macro is a fast terminal text editor written in Rust.
Features an integrated file tree, syntax highlighting for all major
languages, mouse support, and line numbers.
Config file: ~/.config/macro/config.toml

%prep
%setup -q -n macro-code-editor-%{version}

%build
cargo build --release

%install
install -Dm755 target/release/macro %{buildroot}%{_bindir}/macro

%files
%license LICENSE
%{_bindir}/macro

%changelog
* Thu Mar 05 2026 firexrwt <opensource@firexrwt.com> - 0.3.0-1
- Create files and directories from CLI: macro path/to/file.ext
- Ctrl+N in file tree opens new file prompt

* Wed Mar 04 2026 firexrwt <opensource@firexrwt.com> - 0.2.1-1
- Tab key now inserts indent (no longer switches focus)
- Esc in editor switches focus to file tree
- Raise open file limit from 3 to 8

* Wed Mar 04 2026 firexrwt <opensource@firexrwt.com> - 0.2.0-1
- Add tabs: open up to 3 files simultaneously
- Mouse click to switch tabs

* Tue Mar 03 2026 firexrwt <opensource@firexrwt.com> - 0.1.0-1
- Initial release
