# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **GUI**: модалка «Доступ к устройству» больше не мелькает и не исчезает: клик по чипу, открывший её, в том же кадре трактовался как клик-мимо и закрывал окно. Открытие защищено кадром-исключением; клик-мимо/Esc/кнопки работают как прежде. +3 регрессионных теста (gui 252). Проверено живым кликом на реальной мыши.

## [0.1.1] - 2026-11-23

### Docs — R27 «Подготовка к публикации»
- **Fixed**: публичные документы (ADR-001 с пометкой ревизии, identity-registry, snapshot-restore) приведены в соответствие с действующим write-gate `--confirm --write`; упоминания удалённого бланкетного флага нейтрализованы; цитата dry-run-вывода `restore-state` совпадает с фактическим выводом CLI.

### UX — R26 «Публичные тексты и UX»
- **Removed**: бланкетный флаг обхода write-gate уровня 2 из CLI, `GateArgs` и `verify-protocol`. Реальные предохранители сохранены: dry-run по умолчанию, `--confirm --write` на каждую запись, escape для непроверенного устройства `--allow-unverified-device --confirm --write`. `verify-protocol` теперь выполняет только read-only пробы (handshake + дескриптор) и записывает локальный маркер.
- **Changed**: README и AppStream-описание переведены на русский и сокращены (возможности, установка, примеры, безопасность, ограничения); дисклеймер — одна строка неофициальности + MIT «как есть».
- **Changed**: упоминание методологии независимой реализации убрано из публичных текстов (Cargo.toml, AGENTS, ADR-001, descriptor-stability, AUR, CLI long_about).

## [0.1.0] - 2026-11-23

> Не публиковался: первый публичный релиз — 0.1.1 (тег v0.1.0 снят).

### CI hotfix (7ba8102)
- **Fixed**: gate-job падал на `cargo install cargo-deny 0.22.0` (MSRV 1.95 > pinned 1.86) — запинен 0.18.3 (MSRV 1.85, `licenses ok` проверен); в apt-список CI добавлен `fakeroot` (DEPENDS_ON apt-деплоя appimage-builder) — предсказуемый второй отказ build-job закрыт до запуска.

### Релиз — R22 «AppImage-only канал v0.1.0» (принято техлидом: 7288518, c2df31a, 04fa774)
- **Added**: AppImage-диспетчер `panteon-dispatch` (без аргументов GUI, с аргументами CLI) через apprun2 `AppRun.env` override (буквальный shell-AppRun невозможен: app_info.exec требует ELF); udev-правило внутри AppDir; минимальный GitLab CI (gate/build-appimage/release по тегу, без секретов); rust-toolchain.toml pin 1.86.0; scripts/ci-gate.sh + ci-build-appimage.sh; README AppImage-first (прямой `AppImage check-access`, workaround R23 удалён); RELEASE_TEMPLATE.md — текст GitLab Release.
- **Verified**: гейты 199/248 под 1.86.0; AppImage smoke независимо: --version 0.1.0, check-access rc=0 на реальной машине, --print-rule побайтово равен udev-файлу, GUI без аргументов (rc=124 timeout), bundled rule/LICENSE/notices внутри, sha256sum -c OK; чистый ubuntu:22.04 --help rc=0; YAML/bash -n OK; src/ — 0 изменений; тега v0.1.0 нет.
- **Known limitations**: CI-пайплайн реально не запускался (нет доступа к раннеру; скрипты прогнаны локально rc=0); shellcheck не установлен (bash -n); AppImage не байт-воспроизводим — SHA256SUMS генерируется под конкретный артефакт.

### Доступ — R23 «Диагностика доступа к устройству» (принято техлидом: 3f896d3, 18b4eb9, efed1d6, 7439b88)
- **Added**: `device::access` — read-only диагностика (udev-правило отсутствует/устарело/установлено; доступ к hidraw и USB-reset узлу через open/close без ioctl); CLI `check-access [--print-rule]` (rc 0/1, готовая само-contained heredoc-команда установки); GUI — WARNING-чип «Нет доступа к устройству» в статус-баре + модалка с объяснением, командой, «Скопировать команду»/«Перепроверить»; override `PANTEON_UDEV_RULE_DIRS` для тестов.
- **Fixed (платформенные ловушки egui 0.31)**: копирование через `ctx.copy_text` (`PlatformOutput::copied_text` в eframe native — no-op); клик-мимо модалки по фактическому content_rect (`Window::response.clicked_elsewhere()` врёт для нижнего ряда кнопок).
- **Verified**: 199/199 (base, +13), 248/248 (gui, +18); check-access на реальной машине rc=0, --print-rule побайтово равен udev-файлу, пустой rule-dir → rc=1 + heredoc; копирование проверено через X-буфер (580 байт); защищённые блоки и AppBaseline побайтово идентичны; скриншоты docs/screenshots/r23/ (3).
- **Note**: группа input не требуется (uaccess ACL покрывает hidraw + USB-reset); AppImage-точка входа на момент R23 — GUI (R21); прямой `AppImage check-access` появится в R22 (AppRun с пробросом аргументов).

### Релиз — R21 «Упаковка v0.1.0 + правовой блок» (принято техлидом: 3d0ed65, 023eb90, 15ce077, 46306c5)
- **Added**: MIT (полный текст, AS IS заглавными) в корне и крейте; дисклеймеры неофициального статуса/nominative fair use/отказа от гарантий в README (сразу после описания) и AppStream metainfo; нейтральная SVG-иконка без бренда; deny.toml (`cargo deny check licenses` — ok, 422 крейта, copyleft нет); детерминированный THIRD-PARTY-NOTICES (421 зависимость + проект, полные тексты OFL-1.1/Ubuntu-font-1.0 для шрифтов epaint_default_fonts).
- **Added**: упаковка четырёх форматов — cargo-deb (.deb), cargo-generate-rpm (.rpm), AppImage, Flatpak (Platform//24.08, --device=all, БЕЗ --share=network, vendored cargo-sources для offline); udev-правило в /usr/lib/udev/rules.d + postinst reload/trigger; AUR PKGBUILD/.SRCINFO/.install; scripts/build-packages.sh; README «Installation» + починка 13 file:///ссылок.
- **Verified**: payload deb/rpm идентичен (8 позиций: 2 бинарника, udev, desktop, иконка, metainfo, LICENSE/notices/copyright), OEM-файлов 0; smoke dpkg -i ubuntu:22.04 / dnf install fedora — rc=0, ldd чистый; flatpak-бандл установлен, probe --read на живой мыши (PID 0x1440, signature 0x0976); desktop-file-validate/appstreamcli — ok; notices детерминирован (3 прогона — идентичный sha256); гейты 186/230; src/ не тронут (0 файлов).
- **Known limitations**: flatpak export через fallback (build-finish/export/bundle) из-за appstreamcli 0.15 без compose — на CI с AppStream 1.x штатный путь; негативный сценарий udev (без хостовых правил) не воспроизведён без root — документирован; rpm Group: Unspecified (косметика cargo-generate-rpm).

### Подсветка — R20 «Честная шкала яркости и фактические имена палитры» (принято техлидом: 8f56e35, 4ea63e0, 62cae04)
- **Fixed**: шкала яркости переградуирована по живым наблюдениям пользователя + статике OEM (слайдер OEM 0..7, `(7−v)/7`): слайдер GUI теперь wire-уровни 1..=8, левая позиция «0%» (полное отключение, ниббл 7), дубликат 100% (ниббл 8, level 0) удалён из UI; `codec::normalize_brightness_level` (0→8) и `codec::brightness_percent` (1→0, L≥2→L·12.5) — семантика в codec, нормализация во всех входах (sync_baseline, проекция из ledger, загрузка/импорт профилей, snapshot R18 — без ложного «профиль изменён»). Запись R8 «level 0 = нижний кламп» опровергнута живым наблюдением (R20).
- **Fixed**: имена палитры приведены к фактической таблице цветов OEM (подтверждена трейсами и живым наблюдением): Жёлтый, Синий, Розовый, Зелёный, Красный, Голубой, Фиолетовый; ошибочные прототипные имена (Orange/Red/Cyan/Blue на битах 1/2/4/5) и суффиксы «(n th)» удалены; доказательная база по каждому биту зафиксирована в документации протокола.
- **Unchanged**: wire-кодеки (`brightness_to_bank7`/`bank7_to_brightness`, codec::led) и формат Profile (`brightness = N`) побайтово идентичны; профиль с brightness=0 загружается как 8, при перезаписи сохраняется 8 (тот же физический свет). CLI --brightness 0..8 сохранён, help-текст исправлен.
- **Verified**: 186/186 (base, +3 codec), 230/230 (gui, +3 codec +3 gui); render_confirm_dialog/dirty_categories побайтово идентичны; sync_baseline/diff_entries — только нормализация/проценты; характеристизации R19 зелёные; скриншоты docs/screenshots/r20/ (4 + фикстура brightness=0).

### Архитектура — R19 «LED wire-кодек в `codec::led`» (техдолг TD-1 закрыт; принято техлидом: 8c5aa3d, 2c134f3, 6cfbb2b)
- **Changed**: вся LED wire-схема (таблица 14 режимов, энкодер/декодер эффекта, энкодер/декодер маски палитры) перенесена из `gui/mod.rs` в новый `codec::led` — единственный источник правды о LED wire-байтах (AGENTS §4). GUI-каталог `LED_EFFECT_OPTIONS` — только презентация (`label + &'static LedModeSpec`); декодирование возвращает семантический `mode_index`, индекс dropdown находится явным lookup (без допущения равенства).
- **Verified**: чистый рефакторинг, поведение побайтово идентично базе — характеристизационные матрицы 14×8 (эффект), 256 (палитра: all/bits/re-encode), 256 (декод эффекта) сняты с базовой реализации `5c743e5` и независимо перепроверены техлидом симуляцией базового кода; защищённые блоки (`sync_baseline`/`dirty_categories`/`diff_entries`/`render_confirm_dialog`) побайтово идентичны; скриншот «Подсветка» 940×900 машинно идентичен базе (AE=0, RMSE=0).
- **Tests**: 183/183 (base, +10: 5 wire-тестов переехали из gui-gated в базовый гейт + 5 новых характеристизационных), 224/224 (gui, +2 consistency-теста каталога, проекционный тест расширен label-проверкой «6. Комета»); `cargo check --all-targets --features gui` — без warning'ов; дифф только codec/gui + скриншот.
- **Tech debt (backlog, TD-2)**: остаточные wire-знания в GUI вне scope R19 — `gui/expected_state.rs` (`interpret_*` декодируют регистровые значения), упаковка `0x0700 | byte` в `gui/actions.rs`/`gui/app.rs` (контракт `service::resolve_block18` принимает сырые u16), nibble-split `wire_byte >> 4` на границе `plan_led_effect_speed`. Закрывать отдельной итерацией вместе с решением о контракте service (u16 vs семантика).

### Profiles — R18 «Индикация „профиль изменён“» (принято техлидом: 91ac812, 5d8a060)
- **Added**: кнопка «Сохранить» загорается ACCENT, когда конфигурация редактора отличается от сохранённого выбранного профиля (тултип «Конфигурация отличается от сохранённого профиля „X“ — сохраните изменения»); после перезаписи/загрузки/создания гаснет.
- **Architecture**: сравнение `profile_config_eq` — только поля, представляемые редактором (без id/name/button_bindings/second — ловушка model↔codec); снимок `selected_preset_snapshot` (UI-only, вне baseline) ставится в потоках load/create/overwrite, сбрасывается при delete/stale/import; без файлового I/O в кадре. +3 юнит-теста (ложные срабатывания, каждое поле, усечённые списки).
- **Verified**: 173/173 (base), 217/217 (gui); защищённые блоки и модалки R16/R17 побайтово идентичны; скриншоты `docs/screenshots/r18/` (6 + factory-фикстура с нештатными second-каналами — кнопка тихая).

### GUI — пост-R17 правки по запросам пользователя (принято техлидом: c1da254, 24a7a94, 31000fa, fe07960, eee88e7)
- **Fixed**: ложный «Применить» после загрузки профиля, совпадающего с устройством — `project_editor_from_ledger()` проецирует из теневого журнала ВСЕ выражаемые поля (polling, эффект/скорость, палитра, яркость, отдача), baseline = образ устройства; цвета слотов сознательно не проецируются (ADR-002 no-readback, декодирование захваченных блоков ненадёжно) — кастомные цвета честно показываются как pending. +6 тестов, включая негативный контроль.
- **Changed**: «+ Новый профиль» — постоянная кнопка панели (не пункт списка); dropdown содержит только сохранённые профили; пустая папка → селектор disabled, создание доступно. Многоточия убраны из кнопок-действий. Экспорт предзаполняет имя файла выбранным профилем.
- **Fixed**: ссылка «Папка профилей» открывает файловый менеджер напрямую (dolphin/nautilus/… по desktop, fallback xdg-open) — обход сломанной MIME-цепочки, уводившей в KDE Connect; ошибка spawn → статус-бар.
- **Verified**: 173/173 (base), 214/214 (gui); confirm-dialog/sync_baseline побайтово идентичны; дифф только gui.
- **Tech debt (backlog)**: ~~`led_option_from_wire_byte`/`palette_from_mask` декодируют wire-байты в gui/mod.rs~~ — **закрыт в R19 (TD-1)**: кодек перенесён в `codec::led`, GUI маппит семантику → индекс каталога.

### Profiles — R17 «Селектор профилей» (принято техлидом: 25f3595, abb1ad7, 9dffd62)
- **Changed**: вкладка «Профили» — один dropdown (галочка на активном, разделитель, «+ Новый профиль…») + общие кнопки «Сохранить» (перезапись выбранного) и «✕» (удаление); удалены построчные контролы ▸/−, поле имени и контекстная кнопка R16.
- **Added**: модалка «Новый профиль» (создание из текущего состояния включая несохранённые правки; занятое имя → disabled, создание никогда не перезаписывает; двойная проверка имени перед записью); подтверждение переключения при несохранённых правках; stale-selection guard (файл удалён извне → тихий сброс); 4 новых юнит-теста + глиф ✕ в контракте `gui_icons_have_glyphs`.
- **Fixed**: всплывающие поверхности (попап селектора, модалки) сделаны непрозрачными (`CARD_OPAQUE`) — текст под ними больше не просвечивает.
- **Verified**: 173/173 (base), 208/208 (gui); no-op активного пункта и Esc/клик-мимо подтверждены побайтовым сравнением кадров; baseline/sync/dirty/confirm-dialog и модалки R16 побайтово идентичны; скриншоты `docs/screenshots/r17/` (11 шт.).

### Profiles — R16 «Перезапись без удаления» (принято техлидом: 743f3bf, 6bc6cfb, 012d38c)
- **Added**: `store::presets` — весь файловый I/O пресетов (validate_name / preset_path / list / load / save / delete / import_from_path / export_to_path), атомарная запись (tmp + sync_all + rename, очистка tmp при ошибках), 8 юнит-тестов включая path-traversal через все точки входа.
- **Security**: имена профилей валидируются (запрет `/ \ .. :`, управляющих символов, ведущей точки, >64) — устранён path traversal `../../evil` за пределы папки пресетов; перезапись и удаление требуют подтверждения (модалки, Esc).
- **Changed**: загрузка профиля заполняет поле имени, кнопка сохранения контекстная («Обновить „X“» / «+ Сохранить текущий»); из GUI-панели убраны все прямые `fs::`-вызовы; min_inner_size 800→760 (окно 780px стало достижимо).
- **Verified**: 173/173 (base, +8 store) и 204/204 (gui); baseline/sync/confirm-dialog побайтово идентичны; скриншоты `docs/screenshots/r16/` (7 шт., изолированный XDG_STATE_HOME).

### GUI — R15 «Чистый дашборд» (принято техлидом: 11c75cb, cc386e4, abc0cc4, 61ad76b)
- **Fixed**: вертикальный скроллбар на 940×900 и горизонтальный клиппинг на 780×900 устранены (полноширинные карточки, удалена дублирующая колонка «Теневой журнал» с вложенным скроллом); тултипы починены (однократная установка темы/шрифтов вместо per-frame, нативный hover вместо temp-data хака, ширина 260px с переносом); иконка статуса плана ⓘ→ℹ (U+24D8 не покрыт шрифтами — tofu), regression-тест `gui_icons_have_glyphs`.
- **Changed**: primary-поверхность очищена от служебного жаргона — банки/адреса/hex перенесены в тултипы и confirm-диалог (диалог безопасности не изменён, побайтово идентичен); таблица «Параметр|Значение|Обновлено» с человеческими именами и локализованными интерпретациями; время в человеческом формате (`format_human_ts`, юнит-тесты); удалён CLI-флаг `--active-slot` из GUI (поле baseline сохранено); палитра DPI — anchored popup у свотча вместо ломающей Grid 5-й колонки.
- **Verified**: 165/165 (base) и 196/196 (gui) тестов; `cargo check` с/без фичи gui; скриншот-матрикс `docs/screenshots/r15/` (3 ширины × 4 вкладки + тултип/попап/confirm).

### Breaking Changes / Architecture Guards
- **Strict Guard in `execute_plans_with_commit` for Multi-Bank Blocks (ADR-003 §7, §8)**:
  `execute_plans_with_commit` now checks `is_multi_bank_block(addr)` and strictly rejects registers belonging to multi-bank blocks (`0x0010`, `0x0018`, `0x0020`, `0x0028`, `0x0030`, `0x0038`) with `PanteonError::Protocol("multi-bank block requires block session — see ADR-003 §7")`. This eliminates hardware corruption where per-plan commits finalized the entire 8-bank block prematurely.

### Added
- **Block 0x0018 Resolver (`service::resolve_block18`)**:
  - Added resolver synthesizing complete 8-bank `BlockWrite` (addr `0x0018`) preserving neighboring settings (`ACTIVE_DPI_SLOT`, `FIRE_DELAY`, `LED_EFFECT_SPEED`, `LED_COLOR_PALETTE`, `POLLING_RATE`).
  - Values are resolved per-bank: user overrides win; otherwise latest value from `ShadowLedger`; otherwise fallback to `docs/state_reset.toml`; unknown banks with empty ledger and missing in snapshot fall back to `0x0000` with warning.
  - Added `service::is_multi_bank_block(addr)` predicate for multi-bank block detection.
  - Added helpers `execute_polling_rate`, `execute_led_effect_speed`, `execute_led_palette`, and `execute_fire_delay_with_ledger`.

### Added
- **Global LED Brightness Control (R8 / Spec-1)**:
  - Added register model `LED_BRIGHTNESS = (0x07, 0x0038)` and parameterized bank 7 of block 0x0038 with brightness level 0..=8 (active-low PWM formula `nibble = (8 - level) << 4`).
  - Added resolver `service::resolve_block38` and updated `service::plan_color_session` to strictly enforce resolution order: `user override` → `ShadowLedger` → `PanteonError::Protocol("LED brightness unknown, supply --brightness / set in GUI")`.
  - Added `--brightness <0..8>` option to CLI `set-dpi-slots --colors` and `set-led`, with Bank 7 dry-run preview.
  - Added GUI brightness slider (steps 0..8 with quantized percentage labels) in LED and DPI panels.
  - Added `service::execute_led_brightness` helper recording applied brightness to `ShadowLedger`.
- **Polling rate 250 Hz confirmed (R10)**:
  - Register code `0x0703` (Bank 6, Addr `0x0018`) verified via live OEM capture `ctrl_u_polling_500-250.pcap` frame 1995 (`07 18 03 06 18 00 03 07`).
  - Physical verification on hardware via a live USB traffic capture (R10) demonstrated median interrupt packet interval of 3.9999 ms across 2979 samples.
  - Removed hypothesis status from `HZ_250`; all 4 polling rates (125/250/500/1000 Hz) are now fully confirmed. Documented static `bInterval` anomaly in USB descriptor.
- **USB soft-reset required for polling rate change (R10.1)**:
  - Live-apply experiment (R10, live USB capture): writing 125 Hz while 250 Hz active, with Soft-Apply tail delivered and `--no-soft-reset`, left report interval at 4.0 ms — new rate applies only after USB re-enumeration.
  - Added `--no-soft-reset` flag to `set-polling`, `set-active-dpi`, `set-fire-delay` (experimental; default behavior unchanged — soft-reset remains on by default).
  - Triple A/B verification of the full polling ladder on one host: 125/250/1000 Hz measured at 7.999/4.000/1.001 ms median intervals.

### Changed
- **Block-Session Migration for `set-polling`, `set-fire-delay`, `set-active-dpi`**:
  - `cli::write::run_set_polling`, `run_set_fire_delay`, `run_set_active_dpi`, and GUI actions now execute via `execute_block_session` using `resolve_block18` instead of per-plan commits, followed by USB soft-reset re-enumeration.
  - Dry-run previews for `set-polling`, `set-fire-delay`, and `set-active-dpi` now display the complete 17-frame block sequence reflecting preserved ledger banks.

### Fixed
- **LED Brightness Silent Reset Defect & Flash-Semantics (R8)**:
  - Eliminated defect where color writes previously hardcoded `bank 7 @ 0x0038` to `0x0700`, silently resetting the user's configured LED brightness back to 100% on every color change.
  - Hardware verification confirmed brightness ladder: volatile writes to Bank 7 do not override a brightness level already committed into SPI flash. Permanent brightness update requires an explicit `--commit-flash`; subsequent color writes preserve the committed level via ShadowLedger resolution.
- **Active DPI Slot Silent Reset Defect (R9 / Spec-1)**:
  - Eliminated silent fallback of bank 0 (`ACTIVE_DPI_SLOT`) to factory reset snapshot (`0x0718` = slot 1) in `service::resolve_block18`. Resolution order is now strictly: user override → `ShadowLedger` → `PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")`. Empty ledger without override returns protocol error instead of silently corrupting active slot or writing `0x0000`.
  - Added `service::execute_active_dpi_slot` to record applied slot changes into `ShadowLedger`.
  - Added `--active-slot <1..6>` option to CLI (`set-polling`, `set-fire-delay`) and GUI active slot override field, forwarding value as bank 0 user override to `resolve_block18`.
  - Enhanced CLI and GUI dry-run diagnostics to clearly show bank 0 value resolved from ledger.

## [0.2.0] - 2026-11-13

### Breaking Changes
- **`service::plan_slot_colors` superseded by `service::plan_color_session`**:
  `plan_slot_colors(&colors)` is now deprecated in favor of `plan_color_session(main, second) -> Vec<WriteTransactionPlan>`. The deprecated function forwards to `plan_color_session(colors, &DEFAULT_SECOND_COLORS)` to maintain compile-time compatibility, but new code should supply explicit two-state color arrays for accurate wire-level PWM gradient encoding.

### Added
- **Block-Oriented Multi-Bank Session (`BlockWrite` & `execute_block_session`)**:
  - Added `device::BlockWrite` model representing 8 banks per address register block (`BlockWrite { addr: u16, banks: [u16; 8] }`).
  - Added `service::device_service::execute_block_session` and `plan_block_session_frames` matching the exact wire frame sequence from live OEM golden capture `/mnt/VMshared/r-b_200-800.pcap` (single session preamble on `addr 0x0001`, sequential 8-bank data writes followed by single bank commit `mode 0x09` with max bank byte 7 `0x07` and final commit `mode 0x00` per block, followed by soft-apply tail `0x007f`, flash flush `mode 0x10`, and idle `mode 0x00`).
  - Added `--block-session` CLI flag to `set-dpi-slots` executing atomic block-session transmission when authorized under Write-Gate Level 2, with detailed dry-run preview by default.
  - Extended internal session-diff tooling with `--ordered` mode for strict sequential frame-by-frame diffing against golden PCAP/PCAPNG captures.
- **CLI `--colors` option for `set-dpi-slots`**:
  Added optional `--colors <C1> <C2> <C3> <C4> <C5> <C6>` flag accepting 6 hex color values (`#rrggbb` or `rrggbb`) to configure slot colors atomically with DPI points, generating 40 transaction plans (8 DPI + 32 color session plans). Absence of flag preserves current behavior (8 DPI plans).
- **Dynamic Two-State Color Nibble Encoding**:
  - `codec::dpi::encode_slot_colors`: now accepts `(main: &[(u8, u8, u8); 6], second: &[(u8, u8, u8); 6])` and constructs byte values via `(rgb_to_pwm(second[c]) << 4) | (rgb_to_pwm(main[c]) & 0x0f)`.
  - Added `codec::dpi::DEFAULT_SECOND_COLORS` holding the OEM default secondary PWM channel states.
  - Added `codec::dpi::encode_color_block_38(main, second)` to emit 8-bank transactions for register block `0x0038` (bank 2 = `0x0a`, bank 6 = `0x8f`).
  - Added `codec::dpi::encode_color_session(main, second)` generating complete 32-plan block sequences.
  - Added [ADR-003: Two-State Nibble Color Encoding and Full-Bank Transaction Model](docs/adr/ADR-003-dpi-slot-colors-two-state-nibble.md).

### Changed
- **Full 8-Bank Encoding for Block `0x0010` (DPI Slots)**:
  `codec::dpi::encode_dpi_slots` and `service::plan_dpi_slots` expanded from 4 to 8 transaction plans covering all banks 0..7 at address `0x0010`. Banks 0..2 are initialized to `0x00`, bank 4 to constant `0x08` (matching live hardware capture `/mnt/VMshared/r-b_200-800.pcap`), while bank 3 (high-DPI bitmask) and banks 5..7 (lower nibbles) retain existing quantization logic. Full slot update sessions with colors now yield 40 write plans (8 DPI + 32 color session plans).
- **Full 8-Bank Encoding for Block `0x0030`**:
  - Address block `0x0030` now emits all 8 banks rather than only banks 0..1. Banks 2..7 write OEM latch bytes (`0xff, 0x7f, 0x00, 0x00, 0x00, 0x00`).
- **Snapshot Overlay in `snapshot_service::build_full_push_snapshot`**:
  - Ledger overlays are now applied to all `mode == 0x03` frames regardless of whether `(bank, addr)` was previously present in the base profile. Unmatched ledger keys emit a non-fatal warning instead of halting execution.
- **Active DPI Slot Register Tracking**:
  - `cli::dpi::execute_dpi_full_push` accepts `edited_slot: Option<u8>`. Modifying a DPI slot dynamically updates `ACTIVE_DPI_SLOT` (`bank 0 @ 0x0018 = 0x0700 | (0x18 + slot - 1)`).

### Fixed
- Fixed hardware DPI slot color write failure where incorrect constants in `COLOR_CHANNEL_HIGH_NIBBLES` and truncated block writes prevented controller color latching.
- Removed spurious hardcoded entry `(4, 0x0010, 0x0708)` from `execute_dpi_full_push`.
