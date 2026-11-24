# Flatpak — PanteonConfig (`io.gitlab.p4650.pantconfig`)

Манифест: [`io.gitlab.p4650.pantconfig.yml`](io.gitlab.p4650.pantconfig.yml).
Runtime: `org.freedesktop.Platform//24.08`, SDK + `rust-stable`.

## Важно: udev-правила ставятся НА ХОСТЕ

Flatpak **не может** установить udev-правила в хост-систему — песочница не имеет
доступа к `/etc/udev/rules.d` и `/usr/lib/udev/rules.d`. Без правила на хосте
устройство (`/dev/hidraw*`, `/dev/bus/usb/*/*`) недоступно приложению, даже с
`--device=all`, и GUI/CLI покажут «устройство не найдено».

Установите правило одним из способов:

1. **Из пакета deb/rpm** (рекомендуется): `apt install ./panteon-ms89_*.deb` или
   `dnf install ./panteon-ms89-*.rpm` — postinst сам выполнит `udevadm control
   --reload-rules` и `udevadm trigger`.
2. **Вручную из репозитория**:

   ```bash
   sudo install -Dm644 panteon-ms89/udev/99-panteon-ms89.rules \
       /usr/lib/udev/rules.d/99-panteon-ms89.rules
   sudo udevadm control --reload-rules
   sudo udevadm trigger --subsystem-match=hidraw --subsystem-match=usb
   ```

   Правило выдаёт `uaccess` активной сессии пользователя на hidraw
   (`VID 0x30fa`, `PID 0x1440`) и на USB-устройство (для `USBDEVFS_RESET`).
   После установки переподключите мышь, если она была подключена до правила.

Проверка: `ls -l /dev/hidraw*` — владелец/группа должны давать доступ вашей
сессии; `panteon-ms89 list-devices` внутри Flatpak должен видеть устройство.

## Состояние приложения

В песочнице `$XDG_STATE_HOME` указывает внутрь каталога приложения, поэтому
ledger, профили и `verified.toml` лежат в:

```
~/.var/app/io.gitlab.p4650.pantconfig/state/
```

Это ожидаемо: состояние Flatpak-версии изолировано от deb/rpm/AppImage-версий
(там используется `~/.local/state/panteon-ms89`). Не смешивайте профили между
установками без явного экспорта/импорта TOML.

## Сборка и запуск

```bash
# из корня репозитория
flatpak-builder --user --install --force-clean build-dir \
    packaging/flatpak/io.gitlab.p4650.pantconfig.yml

# запуск GUI
flatpak run io.gitlab.p4650.pantconfig

# CLI
flatpak run --command=panteon-ms89 io.gitlab.p4650.pantconfig --help

# сборка переносимого bundle
flatpak-builder --repo=repo build-dir packaging/flatpak/io.gitlab.p4650.pantconfig.yml
flatpak build-bundle repo panteon-ms89.flatpak io.gitlab.p4650.pantconfig
flatpak install --user panteon-ms89.flatpak
```

Требуются установленные `org.freedesktop.Sdk//24.08` и
`org.freedesktop.Sdk.Extension.rust-stable//24.08`:

```bash
flatpak install --user flathub org.freedesktop.Sdk//24.08 \
    org.freedesktop.Sdk.Extension.rust-stable//24.08
```

Сборка использует сеть (cargo скачивает зависимости); у самого приложения
разрешения `--share=network` нет — в рантайме сетевых вызовов нет.

## Разрешения (`finish-args`)

| Разрешение | Зачем |
|---|---|
| `--device=all` | hidraw-узлы и `ioctl(USBDEVFS_RESET)` для soft-reset |
| `--socket=wayland`, `--socket=fallback-x11` | окно GUI (winit/eframe) |
| `--share=ipc` | разделяемая память X11/GL |
| `--talk-name=org.freedesktop.portal.Desktop` | файловые диалоги `rfd`/ashpd |

`--share=network` намеренно отсутствует: приложение не делает сетевых вызовов.

## Ограничения

- Установка udev-правил и прав доступа — ответственность хоста (см. выше).
- Состояние изолировано в `~/.var/app/<id>/state`.
- Flatpak-версия не может перезагрузить правила udev хоста; после обновления
  правила выполните reload/trigger вручную.