# InstNoth

**InstNoth** — симулятор установки программного обеспечения, который ничего не устанавливает. Создаёт реалистичную симуляцию процесса установки на основе скриптов `.instnoth`.

## Возможности

- Симуляция установки без реальных изменений в системе
- Собственный язык описания установки (`.instnoth`)
- Реалистичный вывод в стиле Linux-инсталляторов
- Цветной вывод с прогресс-барами
- Поддержка пользовательских файлов установки
- Несколько примеров установки (Python, Node.js, Docker)

## Установка

### Сборка из исходников

```bash
# Клонируйте репозиторий
git clone https://github.com/example/instnoth.git
cd instnoth

# Соберите проект
cargo build --release

# Запустите
./target/release/instnoth --help
```

### С помощью Cargo

```bash
cargo install --path .
```

## Использование

### Базовое использование

```bash
# Установка одного пакета
instnoth --file examples/python.instnoth

# Установка нескольких пакетов за раз
instnoth --file examples/python.instnoth examples/nodejs.instnoth

# Установка с зависимостями (автоматически)
instnoth --file examples/all.instnoth

# Показать дерево зависимостей
instnoth --file examples/devstack.instnoth --show-deps

# Быстрый режим (без задержек)
instnoth --file examples/nodejs.instnoth --quick

# Подробный вывод
instnoth --file examples/docker.instnoth --verbose

# Все опции вместе
instnoth --file myapp.instnoth --quick --verbose
```

### Параметры командной строки

| Параметр | Сокращение | Описание |
|----------|------------|----------|
| `--file <PATH>...` | `-f` | Путь к файлу(ам) установки `.instnoth` |
| `--quick` | `-q` | Быстрый режим без задержек |
| `--verbose` | `-v` | Подробный вывод с командами |
| `--show-deps` | | Показать дерево зависимостей |
| `--skip-deps` | | Пропустить установку зависимостей |
| `--list-builtin` | | Показать встроенные файлы установки |
| `--help` | `-h` | Показать справку |
| `--version` | `-V` | Показать версию |

## Примеры файлов установки

В директории `examples/` доступны готовые файлы:

| Файл | Описание | Зависимости |
|------|----------|-------------|
| `python.instnoth` | Установка Python 3.12 | — |
| `nodejs.instnoth` | Установка Node.js 20 LTS | — |
| `docker.instnoth` | Установка Docker Engine | — |
| `linux.instnoth` | Установка Arch Linux | — |
| `all.instnoth` | Всё для разработки | python, nodejs, docker |
| `devstack.instnoth` | Полный стек разработчика | linux, python, nodejs, docker |

### Система зависимостей

Файлы могут указывать зависимости через директиву `depends:`:

```instnoth
package: "My App"
version: "1.0.0"
depends: "python.instnoth" "nodejs.instnoth"

phase "Setup" {
    # ...
}
```

При запуске зависимости устанавливаются автоматически в правильном порядке:

```bash
# Установит: Python → Node.js → Docker → All-in-One
instnoth --file examples/all.instnoth

# Показать дерево зависимостей без установки
instnoth --file examples/all.instnoth --show-deps

# Пропустить зависимости
instnoth --file examples/all.instnoth --skip-deps
```

### Пример вывода

```
╔═══════════════════════════════════════════════════════════════════╗
║  InstNoth Installer v1.0                                          ║
╚═══════════════════════════════════════════════════════════════════╝

Package:    Python
Version:    3.12.1
Description:
  Язык программирования Python с pip и стандартной библиотекой
Author:     Python Software Foundation

───────────────────────────────────────────────────────────────────

▶ Подготовка системы
──────────────────────────────────────────────────────
  → Проверка системных требований...
  ? Проверка зависимости: gcc ... OK
  ? Проверка зависимости: make ... OK
  ◉ Прогресс: [███░░░░░░░░░░░░░░░░░░░░░░░░░░░] 10%
  ✓ Системные требования выполнены

▶ Загрузка компонентов
──────────────────────────────────────────────────────
  → Подключение к python.org...
  ⬇ Загрузка: https://www.python.org/ftp/python/3.12.1/Python-3.12.1.tar.xz
    [████████████████████████████████████████] 20480/20480 (0s)
    ✓ Загружено: 20480 байт
...

═══════════════════════════════════════════════════════════════════
  ✓ Python установлен успешно!
═══════════════════════════════════════════════════════════════════
```

## Создание своего файла установки

Создайте файл с расширением `.instnoth`:

```instnoth
# myapp.instnoth

package: "MyApp"
version: "1.0.0"
description: "Моё приложение"
author: "Developer"

phase "Подготовка" {
    message "Проверка системы..."
    check_dep "gcc"
    progress 20
}

phase "Загрузка" {
    download "https://example.com/myapp.tar.gz" size=10240
    progress 50
}

phase "Установка" {
    extract "/tmp/myapp.tar.gz" to="/opt/myapp"
    create_dir "/etc/myapp"
    copy_file "/opt/myapp/bin/myapp" to="/usr/local/bin/myapp"
    set_permission "/usr/local/bin/myapp" mode="755"
    progress 90
}

phase "Завершение" {
    cleanup
    progress 100
    success "MyApp установлен!"
}
```

Запустите:

```bash
instnoth --file myapp.instnoth
```

## Документация формата

Подробная документация формата `.instnoth` доступна в файле [INSTNOTH.md](INSTNOTH.md).

### Краткий список команд

#### Базовые команды
| Команда | Описание |
|---------|----------|
| `message "текст"` | Информационное сообщение |
| `success "текст"` | Сообщение об успехе |
| `warning "текст"` | Предупреждение |
| `error "текст"` | Ошибка |
| `delay N` | Пауза N миллисекунд |
| `progress N` | Установка прогресса (0-100) |

#### Детекция системы (случайные данные)
| Команда | Описание |
|---------|----------|
| `detect_cpu` | Определение CPU |
| `detect_memory` | Определение RAM |
| `detect_disk` | Определение накопителей |
| `detect_gpu` | Определение видеокарты |
| `detect_network` | Определение сети |
| `detect_os` | Определение ОС |
| `detect_kernel` | Версия ядра |
| `detect_bios` | Информация BIOS/UEFI |
| `scan_hardware` | Полное сканирование |
| `detect_drivers` | Определение драйверов |

#### Тесты и бенчмарки
| Команда | Описание |
|---------|----------|
| `run_test "имя" duration=N` | Запуск теста |
| `test_hardware "компонент"` | Тест компонента |
| `benchmark_cpu` | CPU бенчмарк |
| `benchmark_memory` | RAM бенчмарк |
| `benchmark_disk` | Тест диска |

#### Работа с ядром
| Команда | Описание |
|---------|----------|
| `load_module "модуль"` | Загрузка модуля |
| `unload_module "модуль"` | Выгрузка модуля |
| `update_initramfs` | Обновление initramfs |
| `update_grub` | Обновление GRUB |
| `compile_kernel "версия"` | Компиляция ядра |

#### Диски и разделы
| Команда | Описание |
|---------|----------|
| `create_partition "устр" size="размер"` | Создание раздела |
| `format "устр" fs="тип"` | Форматирование |
| `mount "устр" to="точка"` | Монтирование |
| `unmount "точка"` | Размонтирование |
| `generate_fstab` | Генерация fstab |

#### Настройка системы
| Команда | Описание |
|---------|----------|
| `set_hostname "имя"` | Установка hostname |
| `set_timezone "зона"` | Часовой пояс |
| `set_locale "локаль"` | Локаль |
| `create_user "имя" groups="группы"` | Создание пользователя |
| `set_password "пользователь"` | Установка пароля |

#### Сервисы
| Команда | Описание |
|---------|----------|
| `enable_service "сервис"` | Включение |
| `disable_service "сервис"` | Отключение |
| `start_service "сервис"` | Запуск |
| `stop_service "сервис"` | Остановка |
| `install_bootloader "устр"` | Установка GRUB |

#### Пакеты и файлы
| Команда | Описание |
|---------|----------|
| `install_packages "список"` | Установка пакетов |
| `install_driver "драйвер"` | Установка драйвера |
| `update_system` | Обновление системы |
| `download "url" size=N` | Загрузка файла |
| `extract "from" to="to"` | Распаковка архива |
| `create_dir "path"` | Создание директории |
| `copy_file "from" to="to"` | Копирование файла |
| `write_config "path" content="..."` | Запись конфига |
| `check_integrity "путь"` | Проверка целостности |
| `verify_signature "файл"` | Проверка подписи |

## Зачем это нужно?

- **Демонстрации** — показ процесса установки без реальных изменений
- **Обучение** — понимание этапов установки ПО
- **Тестирование UI** — проверка отображения прогресса
- **Развлечение** — создание "фейковых" установщиков
- **Прототипирование** — планирование реальных установщиков

## Лицензия

MIT License

## Авторы

InstNoth Team
