# Сборка и публикация в Fedora COPR

## Подготовка исходников

```bash
# 1. Собрать вендор-архив (нужен для offline сборки в Koji)
cd macro-editor/
cargo vendor
tar czf vendor.tar.gz vendor/
rm -rf vendor/

# 2. Создать source tarball
cd ..
tar czf macro-editor-0.1.0.tar.gz macro-editor/

# Итого нужны два файла:
#   macro-editor-0.1.0.tar.gz
#   vendor.tar.gz
```

## Публикация в COPR

```bash
# Установить copr-cli
sudo dnf install copr-cli

# Настроить токен: https://copr.fedorainfracloud.org/api/
# Положить ~/.config/copr

# Создать проект в COPR (один раз)
copr-cli create macro-editor --chroot fedora-rawhide-x86_64

# Собрать из spec + srpm
rpmbuild -bs packaging/macro-editor.spec \
  --define "_sourcedir $(pwd)/packaging" \
  --define "_srcrpmdir $(pwd)"

copr-cli build macro-editor macro-editor-0.1.0-1.*.src.rpm
```

## Установка из COPR (для пользователей)

```bash
sudo dnf copr enable firexrwt/macro-editor
sudo dnf install macro-editor
```
