use clap::Parser as ClapParser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

/// InstNoth - Симулятор установки, который ничего не устанавливает
#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Путь к файлам установки (.instnoth) - можно указать несколько
    #[arg(short, long, num_args = 1..)]
    file: Option<Vec<PathBuf>>,

    /// Режим быстрой установки (без задержек)
    #[arg(short, long, default_value_t = false)]
    quick: bool,

    /// Подробный вывод
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Показать список встроенных файлов установки
    #[arg(long, default_value_t = false)]
    list_builtin: bool,

    /// Пропустить установку зависимостей
    #[arg(long, default_value_t = false)]
    skip_deps: bool,

    /// Показать дерево зависимостей без установки
    #[arg(long, default_value_t = false)]
    show_deps: bool,
}

// ============== Структуры данных ==============

#[derive(Debug, Clone)]
struct Package {
    name: String,
    version: String,
    description: String,
    author: String,
    depends: Vec<String>,
    phases: Vec<Phase>,
    file_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct Phase {
    name: String,
    commands: Vec<Command>,
}

#[derive(Debug, Clone)]
enum Command {
    Message(String),
    Delay(u64),
    Progress(u8),
    CreateDir(String),
    Download { url: String, size: u64 },
    Extract { from: String, to: String },
    InstallDep { name: String, version: String },
    Configure { key: String, value: String },
    Cleanup,
    Success(String),
    Error(String),
    Warning(String),
    CopyFile { from: String, to: String },
    Symlink { from: String, to: String },
    SetPermission { path: String, mode: String },
    RunScript(String),
    CheckDep(String),
    WriteConfig { path: String, content: String },
    DetectCpu,
    DetectMemory,
    DetectDisk,
    DetectGpu,
    DetectNetwork,
    DetectOs,
    DetectKernel,
    DetectBios,
    RunTest { name: String, duration: u64 },
    LoadKernelModule(String),
    UnloadKernelModule(String),
    UpdateInitramfs,
    UpdateGrub,
    MountPartition { device: String, mount_point: String },
    UnmountPartition(String),
    FormatPartition { device: String, fs_type: String },
    CreatePartition { device: String, size: String },
    SetHostname(String),
    SetTimezone(String),
    SetLocale(String),
    CreateUser { username: String, groups: String },
    SetPassword(String),
    EnableService(String),
    DisableService(String),
    StartService(String),
    StopService(String),
    InstallBootloader(String),
    GenerateFstab,
    CheckIntegrity(String),
    VerifySignature(String),
    CompileKernel { version: String },
    InstallPackages(String),
    UpdateSystem,
    SyncTime,
    TestHardware(String),
    BenchmarkCpu,
    BenchmarkMemory,
    BenchmarkDisk,
    NetworkConfig { interface: String, config: String },
    FirewallRule(String),
    ScanHardware,
    DetectDrivers,
    InstallDriver(String),
}

// ============== Парсер ==============

struct InstnothParser {
    content: String,
    file_path: Option<PathBuf>,
}

impl InstnothParser {
    fn new(content: String) -> Self {
        Self { content, file_path: None }
    }

    fn with_path(content: String, path: PathBuf) -> Self {
        Self { content, file_path: Some(path) }
    }

    fn parse(&mut self) -> Result<Package, String> {
        let mut package = Package {
            name: String::new(),
            version: String::new(),
            description: String::new(),
            author: String::new(),
            depends: Vec::new(),
            phases: Vec::new(),
            file_path: self.file_path.clone(),
        };

        let lines: Vec<&str> = self.content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            if line.is_empty() || line.starts_with('#') {
                i += 1;
                continue;
            }

            if line.starts_with("package:") {
                package.name = Self::extract_quoted_value(line)?;
            } else if line.starts_with("version:") {
                package.version = Self::extract_quoted_value(line)?;
            } else if line.starts_with("description:") {
                package.description = Self::extract_quoted_value(line)?;
            } else if line.starts_with("author:") {
                package.author = Self::extract_quoted_value(line)?;
            } else if line.starts_with("depends:") {
                // Парсим зависимости: depends: "file1.instnoth" "file2.instnoth"
                // или depends: "file1.instnoth, file2.instnoth"
                let deps_str = &line["depends:".len()..];
                package.depends = Self::parse_depends(deps_str);
            } else if line.starts_with("phase") {
                let phase_name = Self::extract_phase_name(line)?;
                let mut phase = Phase {
                    name: phase_name,
                    commands: Vec::new(),
                };

                if !line.contains('{') {
                    i += 1;
                    while i < lines.len() && !lines[i].contains('{') {
                        i += 1;
                    }
                }
                i += 1;

                while i < lines.len() {
                    let cmd_line = lines[i].trim();
                    if cmd_line == "}" || cmd_line.starts_with('}') {
                        break;
                    }
                    if !cmd_line.is_empty() && !cmd_line.starts_with('#') {
                        if let Ok(cmd) = self.parse_command(cmd_line) {
                            phase.commands.push(cmd);
                        }
                    }
                    i += 1;
                }

                package.phases.push(phase);
            }

            i += 1;
        }

        if package.name.is_empty() {
            return Err("Не указано имя пакета".to_string());
        }

        Ok(package)
    }

    fn parse_depends(deps_str: &str) -> Vec<String> {
        let mut deps = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;

        for c in deps_str.chars() {
            match c {
                '"' => {
                    if in_quotes {
                        if !current.trim().is_empty() {
                            deps.push(current.trim().to_string());
                        }
                        current = String::new();
                    }
                    in_quotes = !in_quotes;
                }
                ',' if !in_quotes => {
                    if !current.trim().is_empty() {
                        deps.push(current.trim().to_string());
                    }
                    current = String::new();
                }
                _ if in_quotes => {
                    current.push(c);
                }
                _ => {}
            }
        }

        if !current.trim().is_empty() {
            deps.push(current.trim().to_string());
        }

        deps
    }

    fn extract_quoted_value(line: &str) -> Result<String, String> {
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                return Ok(line[start + 1..start + 1 + end].to_string());
            }
        }
        Err(format!("Не удалось извлечь значение из: {}", line))
    }

    fn extract_phase_name(line: &str) -> Result<String, String> {
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                return Ok(line[start + 1..start + 1 + end].to_string());
            }
        }
        Err("Не удалось извлечь имя фазы".to_string())
    }

    fn parse_command(&self, line: &str) -> Result<Command, String> {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        let cmd = parts[0];
        let args = if parts.len() > 1 { parts[1] } else { "" };

        match cmd {
            "message" => Ok(Command::Message(Self::extract_quoted_value(line)?)),
            "delay" => {
                let ms: u64 = args.trim().parse().unwrap_or(100);
                Ok(Command::Delay(ms))
            }
            "progress" => {
                let pct: u8 = args.trim().parse().unwrap_or(0);
                Ok(Command::Progress(pct))
            }
            "create_dir" => Ok(Command::CreateDir(Self::extract_quoted_value(line)?)),
            "download" => {
                let url = Self::extract_quoted_value(line)?;
                let size = self.extract_param(args, "size").unwrap_or(1024);
                Ok(Command::Download { url, size })
            }
            "extract" => {
                let from = Self::extract_quoted_value(line)?;
                let to = self.extract_string_param(args, "to").unwrap_or_default();
                Ok(Command::Extract { from, to })
            }
            "install_dep" => {
                let name = Self::extract_quoted_value(line)?;
                let version = self.extract_string_param(args, "version").unwrap_or("latest".to_string());
                Ok(Command::InstallDep { name, version })
            }
            "configure" => {
                let key = self.extract_string_param(args, "key").unwrap_or_default();
                let value = self.extract_string_param(args, "value").unwrap_or_default();
                Ok(Command::Configure { key, value })
            }
            "cleanup" => Ok(Command::Cleanup),
            "success" => Ok(Command::Success(Self::extract_quoted_value(line)?)),
            "error" => Ok(Command::Error(Self::extract_quoted_value(line)?)),
            "warning" => Ok(Command::Warning(Self::extract_quoted_value(line)?)),
            "copy_file" => {
                let from = Self::extract_quoted_value(line)?;
                let to = self.extract_string_param(args, "to").unwrap_or_default();
                Ok(Command::CopyFile { from, to })
            }
            "symlink" => {
                let from = Self::extract_quoted_value(line)?;
                let to = self.extract_string_param(args, "to").unwrap_or_default();
                Ok(Command::Symlink { from, to })
            }
            "set_permission" => {
                let path = Self::extract_quoted_value(line)?;
                let mode = self.extract_string_param(args, "mode").unwrap_or("755".to_string());
                Ok(Command::SetPermission { path, mode })
            }
            "run_script" => Ok(Command::RunScript(Self::extract_quoted_value(line)?)),
            "check_dep" => Ok(Command::CheckDep(Self::extract_quoted_value(line)?)),
            "write_config" => {
                let path = Self::extract_quoted_value(line)?;
                let content = self.extract_string_param(args, "content").unwrap_or_default();
                Ok(Command::WriteConfig { path, content })
            }
            "detect_cpu" => Ok(Command::DetectCpu),
            "detect_memory" => Ok(Command::DetectMemory),
            "detect_disk" => Ok(Command::DetectDisk),
            "detect_gpu" => Ok(Command::DetectGpu),
            "detect_network" => Ok(Command::DetectNetwork),
            "detect_os" => Ok(Command::DetectOs),
            "detect_kernel" => Ok(Command::DetectKernel),
            "detect_bios" => Ok(Command::DetectBios),
            "run_test" => {
                let name = Self::extract_quoted_value(line)?;
                let duration = self.extract_param(args, "duration").unwrap_or(1000);
                Ok(Command::RunTest { name, duration })
            }
            "load_module" => Ok(Command::LoadKernelModule(Self::extract_quoted_value(line)?)),
            "unload_module" => Ok(Command::UnloadKernelModule(Self::extract_quoted_value(line)?)),
            "update_initramfs" => Ok(Command::UpdateInitramfs),
            "update_grub" => Ok(Command::UpdateGrub),
            "mount" => {
                let device = Self::extract_quoted_value(line)?;
                let mount_point = self.extract_string_param(args, "to").unwrap_or_default();
                Ok(Command::MountPartition { device, mount_point })
            }
            "unmount" => Ok(Command::UnmountPartition(Self::extract_quoted_value(line)?)),
            "format" => {
                let device = Self::extract_quoted_value(line)?;
                let fs_type = self.extract_string_param(args, "fs").unwrap_or("ext4".to_string());
                Ok(Command::FormatPartition { device, fs_type })
            }
            "create_partition" => {
                let device = Self::extract_quoted_value(line)?;
                let size = self.extract_string_param(args, "size").unwrap_or("100%".to_string());
                Ok(Command::CreatePartition { device, size })
            }
            "set_hostname" => Ok(Command::SetHostname(Self::extract_quoted_value(line)?)),
            "set_timezone" => Ok(Command::SetTimezone(Self::extract_quoted_value(line)?)),
            "set_locale" => Ok(Command::SetLocale(Self::extract_quoted_value(line)?)),
            "create_user" => {
                let username = Self::extract_quoted_value(line)?;
                let groups = self.extract_string_param(args, "groups").unwrap_or("users".to_string());
                Ok(Command::CreateUser { username, groups })
            }
            "set_password" => Ok(Command::SetPassword(Self::extract_quoted_value(line)?)),
            "enable_service" => Ok(Command::EnableService(Self::extract_quoted_value(line)?)),
            "disable_service" => Ok(Command::DisableService(Self::extract_quoted_value(line)?)),
            "start_service" => Ok(Command::StartService(Self::extract_quoted_value(line)?)),
            "stop_service" => Ok(Command::StopService(Self::extract_quoted_value(line)?)),
            "install_bootloader" => Ok(Command::InstallBootloader(Self::extract_quoted_value(line)?)),
            "generate_fstab" => Ok(Command::GenerateFstab),
            "check_integrity" => Ok(Command::CheckIntegrity(Self::extract_quoted_value(line)?)),
            "verify_signature" => Ok(Command::VerifySignature(Self::extract_quoted_value(line)?)),
            "compile_kernel" => {
                let version = Self::extract_quoted_value(line)?;
                Ok(Command::CompileKernel { version })
            }
            "install_packages" => Ok(Command::InstallPackages(Self::extract_quoted_value(line)?)),
            "update_system" => Ok(Command::UpdateSystem),
            "sync_time" => Ok(Command::SyncTime),
            "test_hardware" => Ok(Command::TestHardware(Self::extract_quoted_value(line)?)),
            "benchmark_cpu" => Ok(Command::BenchmarkCpu),
            "benchmark_memory" => Ok(Command::BenchmarkMemory),
            "benchmark_disk" => Ok(Command::BenchmarkDisk),
            "network_config" => {
                let interface = Self::extract_quoted_value(line)?;
                let config = self.extract_string_param(args, "config").unwrap_or("dhcp".to_string());
                Ok(Command::NetworkConfig { interface, config })
            }
            "firewall_rule" => Ok(Command::FirewallRule(Self::extract_quoted_value(line)?)),
            "scan_hardware" => Ok(Command::ScanHardware),
            "detect_drivers" => Ok(Command::DetectDrivers),
            "install_driver" => Ok(Command::InstallDriver(Self::extract_quoted_value(line)?)),
            _ => Err(format!("Неизвестная команда: {}", cmd)),
        }
    }

    fn extract_param(&self, args: &str, name: &str) -> Option<u64> {
        let pattern = format!("{}=", name);
        if let Some(pos) = args.find(&pattern) {
            let start = pos + pattern.len();
            let rest = &args[start..];
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest[..end].parse().ok()
        } else {
            None
        }
    }

    fn extract_string_param(&self, args: &str, name: &str) -> Option<String> {
        let pattern = format!("{}=\"", name);
        if let Some(pos) = args.find(&pattern) {
            let start = pos + pattern.len();
            let rest = &args[start..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        None
    }
}

// ============== Генераторы случайных данных ==============

struct RandomSystemInfo;

impl RandomSystemInfo {
    fn cpu() -> (&'static str, &'static str, u32, u32) {
        let mut rng = rand::thread_rng();
        let cpus = [
            ("Intel", "Core i9-13900K", 24, 5800),
            ("Intel", "Core i7-12700K", 12, 5000),
            ("Intel", "Core i5-13600K", 14, 5100),
            ("Intel", "Xeon E5-2699 v4", 22, 3600),
            ("AMD", "Ryzen 9 7950X", 16, 5700),
            ("AMD", "Ryzen 7 7800X3D", 8, 5000),
            ("AMD", "Ryzen 5 7600X", 6, 5300),
            ("AMD", "EPYC 7742", 64, 3400),
            ("AMD", "Threadripper 3990X", 64, 4300),
            ("Apple", "M2 Ultra", 24, 3500),
        ];
        let idx = rng.gen_range(0..cpus.len());
        cpus[idx]
    }

    fn memory() -> (u64, &'static str, u32) {
        let mut rng = rand::thread_rng();
        let configs = [
            (8, "DDR4", 2666),
            (16, "DDR4", 3200),
            (32, "DDR4", 3600),
            (32, "DDR5", 4800),
            (64, "DDR5", 5600),
            (128, "DDR5", 6000),
            (16, "DDR5", 5200),
            (64, "DDR4", 3200),
        ];
        let idx = rng.gen_range(0..configs.len());
        configs[idx]
    }

    fn disk() -> (&'static str, &'static str, u64, &'static str) {
        let mut rng = rand::thread_rng();
        let disks = [
            ("Samsung", "990 PRO", 2000, "NVMe"),
            ("Samsung", "870 EVO", 1000, "SATA"),
            ("WD", "Black SN850X", 2000, "NVMe"),
            ("WD", "Blue SN570", 500, "NVMe"),
            ("Seagate", "Barracuda", 2000, "HDD"),
            ("Crucial", "MX500", 1000, "SATA"),
            ("Kingston", "NV2", 1000, "NVMe"),
            ("Toshiba", "X300", 4000, "HDD"),
            ("Intel", "Optane 905P", 960, "NVMe"),
        ];
        let idx = rng.gen_range(0..disks.len());
        disks[idx]
    }

    fn gpu() -> (&'static str, &'static str, u32) {
        let mut rng = rand::thread_rng();
        let gpus = [
            ("NVIDIA", "GeForce RTX 4090", 24),
            ("NVIDIA", "GeForce RTX 4080", 16),
            ("NVIDIA", "GeForce RTX 4070 Ti", 12),
            ("NVIDIA", "GeForce RTX 3080", 10),
            ("AMD", "Radeon RX 7900 XTX", 24),
            ("AMD", "Radeon RX 7800 XT", 16),
            ("AMD", "Radeon RX 6800", 16),
            ("Intel", "Arc A770", 16),
            ("Intel", "Arc A380", 6),
            ("NVIDIA", "Quadro RTX 8000", 48),
        ];
        let idx = rng.gen_range(0..gpus.len());
        gpus[idx]
    }

    fn network() -> (&'static str, &'static str, &'static str) {
        let mut rng = rand::thread_rng();
        let nics = [
            ("Intel", "I225-V 2.5GbE", "2.5 Gbps"),
            ("Intel", "X710 10GbE", "10 Gbps"),
            ("Realtek", "RTL8125", "2.5 Gbps"),
            ("Realtek", "RTL8111", "1 Gbps"),
            ("Broadcom", "BCM57416", "10 Gbps"),
            ("Mellanox", "ConnectX-6", "100 Gbps"),
            ("Intel", "Wi-Fi 6E AX211", "2.4 Gbps"),
            ("Qualcomm", "Atheros AR9485", "300 Mbps"),
        ];
        let idx = rng.gen_range(0..nics.len());
        nics[idx]
    }

    fn bios() -> (&'static str, &'static str, &'static str) {
        let mut rng = rand::thread_rng();
        let bioses = [
            ("American Megatrends", "UEFI", "3.5.2"),
            ("Phoenix", "UEFI", "2.1.0"),
            ("Insyde", "UEFI", "5.0"),
            ("Award", "Legacy BIOS", "6.0"),
            ("AMI", "Aptio V", "1.24"),
            ("Dell", "UEFI", "2.8.1"),
            ("HP", "UEFI", "F.47"),
            ("Lenovo", "UEFI", "N24ET82W"),
        ];
        let idx = rng.gen_range(0..bioses.len());
        bioses[idx]
    }

    fn kernel() -> &'static str {
        let mut rng = rand::thread_rng();
        let kernels = [
            "6.6.8-arch1-1",
            "6.5.0-14-generic",
            "6.1.52-gentoo",
            "5.15.0-91-generic",
            "6.6.6-200.fc39.x86_64",
            "6.4.12-1-MANJARO",
            "5.10.0-27-amd64",
            "6.2.16-300.fc38.x86_64",
        ];
        let idx = rng.gen_range(0..kernels.len());
        kernels[idx]
    }

    fn os() -> (&'static str, &'static str) {
        let mut rng = rand::thread_rng();
        let systems = [
            ("Ubuntu", "22.04.3 LTS (Jammy Jellyfish)"),
            ("Fedora", "39 (Workstation Edition)"),
            ("Debian", "12 (Bookworm)"),
            ("Arch Linux", "Rolling Release"),
            ("openSUSE", "Tumbleweed"),
            ("Linux Mint", "21.2 (Victoria)"),
            ("Pop!_OS", "22.04 LTS"),
            ("Manjaro", "23.1 (Vulcan)"),
            ("CentOS Stream", "9"),
            ("Rocky Linux", "9.3"),
        ];
        let idx = rng.gen_range(0..systems.len());
        systems[idx]
    }

    fn mac_address() -> String {
        let mut rng = rand::thread_rng();
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>(),
            rng.gen::<u8>(), rng.gen::<u8>(), rng.gen::<u8>()
        )
    }

    fn ip_address() -> String {
        let mut rng = rand::thread_rng();
        format!(
            "192.168.{}.{}",
            rng.gen_range(0..255),
            rng.gen_range(1..254)
        )
    }
}

// ============== Менеджер зависимостей ==============

struct DependencyManager {
    base_path: PathBuf,
    installed: HashSet<String>,
}

impl DependencyManager {
    fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            installed: HashSet::new(),
        }
    }

    fn resolve_path(&self, dep_path: &str) -> PathBuf {
        let path = Path::new(dep_path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        }
    }

    fn load_package(&self, path: &Path) -> Result<Package, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Не удалось прочитать файл {:?}: {}", path, e))?;
        
        let mut parser = InstnothParser::with_path(content, path.to_path_buf());
        parser.parse()
    }

    fn get_install_order(&mut self, packages: &[Package]) -> Result<Vec<Package>, String> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();

        for pkg in packages {
            self.visit_package(pkg, &mut order, &mut visited, &mut in_stack)?;
        }

        Ok(order)
    }

    fn visit_package(
        &self,
        pkg: &Package,
        order: &mut Vec<Package>,
        visited: &mut HashSet<String>,
        in_stack: &mut HashSet<String>,
    ) -> Result<(), String> {
        let pkg_id = pkg.name.clone();

        if in_stack.contains(&pkg_id) {
            return Err(format!("Обнаружена циклическая зависимость: {}", pkg_id));
        }

        if visited.contains(&pkg_id) {
            return Ok(());
        }

        in_stack.insert(pkg_id.clone());

        // Обрабатываем зависимости
        for dep_path in &pkg.depends {
            let full_path = self.resolve_path(dep_path);
            if let Ok(dep_pkg) = self.load_package(&full_path) {
                self.visit_package(&dep_pkg, order, visited, in_stack)?;
            } else {
                eprintln!("{} Не удалось загрузить зависимость: {}", "⚠".yellow(), dep_path);
            }
        }

        in_stack.remove(&pkg_id);
        visited.insert(pkg_id);
        order.push(pkg.clone());

        Ok(())
    }

    fn mark_installed(&mut self, name: &str) {
        self.installed.insert(name.to_string());
    }

    fn is_installed(&self, name: &str) -> bool {
        self.installed.contains(name)
    }
}

fn show_dependency_tree(pkg: &Package, dep_manager: &DependencyManager, indent: usize, visited: &mut HashSet<String>) {
    let prefix = "  ".repeat(indent);
    let marker = if indent == 0 { "📦" } else { "├─" };
    
    println!("{}{} {} (v{})", prefix, marker, pkg.name.cyan().bold(), pkg.version);
    
    if visited.contains(&pkg.name) {
        println!("{}  └─ {}", prefix, "(уже показан)".dimmed());
        return;
    }
    visited.insert(pkg.name.clone());
    
    for (i, dep_path) in pkg.depends.iter().enumerate() {
        let full_path = dep_manager.resolve_path(dep_path);
        let is_last = i == pkg.depends.len() - 1;
        let branch = if is_last { "└─" } else { "├─" };
        
        if let Ok(dep_pkg) = dep_manager.load_package(&full_path) {
            println!("{}  {} {}", prefix, branch, dep_path.yellow());
            show_dependency_tree(&dep_pkg, dep_manager, indent + 2, visited);
        } else {
            println!("{}  {} {} {}", prefix, branch, dep_path.yellow(), "(не найден)".red());
        }
    }
}

// ============== Симулятор ==============

struct Simulator {
    quick_mode: bool,
    verbose: bool,
    progress: u8,
}

impl Simulator {
    fn new(quick_mode: bool, verbose: bool) -> Self {
        Self {
            quick_mode,
            verbose,
            progress: 0,
        }
    }

    fn run(&mut self, package: &Package) -> Result<(), String> {
        self.print_header(package);

        for phase in &package.phases {
            self.run_phase(phase)?;
        }

        self.print_footer(package);
        Ok(())
    }

    fn print_header(&self, package: &Package) {
        println!();
        println!("{}", "╔═══════════════════════════════════════════════════════════════════╗".cyan());
        println!("{}", "║  InstNoth Installer v1.0                                          ║".cyan());
        println!("{}", "╚═══════════════════════════════════════════════════════════════════╝".cyan());
        println!();
        println!("{}:    {}", "Package".green().bold(), package.name.white().bold());
        println!("{}:    {}", "Version".green().bold(), package.version.white());
        if !package.description.is_empty() {
            println!("{}:", "Description".green().bold());
            println!("  {}", package.description.white().dimmed());
        }
        if !package.author.is_empty() {
            println!("{}:     {}", "Author".green().bold(), package.author.white());
        }
        if !package.depends.is_empty() {
            println!("{}:   {}", "Depends".green().bold(), package.depends.join(", ").yellow());
        }
        println!();
        println!("{}", "───────────────────────────────────────────────────────────────────".dimmed());
        println!();
    }

    fn print_footer(&self, package: &Package) {
        println!();
        println!("{}", "═══════════════════════════════════════════════════════════════════".green());
        println!("{}", format!("  {} {} установлен успешно!", "✓".green().bold(), package.name).green());
        println!("{}", "═══════════════════════════════════════════════════════════════════".green());
        println!();
    }

    fn run_phase(&mut self, phase: &Phase) -> Result<(), String> {
        println!();
        println!("{} {}", "▶".blue().bold(), phase.name.blue().bold());
        println!("{}", "─".repeat(50).dimmed());

        for cmd in &phase.commands {
            self.execute_command(cmd)?;
        }

        Ok(())
    }

    fn execute_command(&mut self, cmd: &Command) -> Result<(), String> {
        match cmd {
            Command::Message(msg) => {
                println!("  {} {}", "→".dimmed(), msg);
            }
            Command::Delay(ms) => {
                if !self.quick_mode {
                    thread::sleep(Duration::from_millis(*ms));
                }
            }
            Command::Progress(pct) => {
                self.progress = *pct;
                self.show_progress_bar(*pct);
            }
            Command::CreateDir(path) => {
                self.simulate_operation(&format!("Создание директории: {}", path), 200)?;
                if self.verbose {
                    println!("    {} mkdir -p {}", "$".dimmed(), path.yellow());
                }
            }
            Command::Download { url, size } => {
                self.simulate_download(url, *size)?;
            }
            Command::Extract { from, to } => {
                self.simulate_extraction(from, to)?;
            }
            Command::InstallDep { name, version } => {
                self.simulate_dep_install(name, version)?;
            }
            Command::Configure { key, value } => {
                println!("  {} Конфигурация: {}={}", "⚙".cyan(), key.yellow(), value.green());
                if !self.quick_mode {
                    thread::sleep(Duration::from_millis(100));
                }
            }
            Command::Cleanup => {
                self.simulate_operation("Очистка временных файлов...", 300)?;
                if self.verbose {
                    println!("    {} rm -rf /tmp/instnoth_*", "$".dimmed());
                }
            }
            Command::Success(msg) => {
                println!("  {} {}", "✓".green().bold(), msg.green());
            }
            Command::Error(msg) => {
                println!("  {} {}", "✗".red().bold(), msg.red());
            }
            Command::Warning(msg) => {
                println!("  {} {}", "⚠".yellow().bold(), msg.yellow());
            }
            Command::CopyFile { from, to } => {
                println!("  {} Копирование: {} → {}", "📄".normal(), from.dimmed(), to.cyan());
                if self.verbose {
                    println!("    {} cp {} {}", "$".dimmed(), from, to);
                }
                if !self.quick_mode {
                    thread::sleep(Duration::from_millis(150));
                }
            }
            Command::Symlink { from, to } => {
                println!("  {} Создание ссылки: {} → {}", "🔗".normal(), from.dimmed(), to.cyan());
                if self.verbose {
                    println!("    {} ln -s {} {}", "$".dimmed(), from, to);
                }
                if !self.quick_mode {
                    thread::sleep(Duration::from_millis(100));
                }
            }
            Command::SetPermission { path, mode } => {
                println!("  {} Установка прав {} для {}", "🔐".normal(), mode.yellow(), path.cyan());
                if self.verbose {
                    println!("    {} chmod {} {}", "$".dimmed(), mode, path);
                }
                if !self.quick_mode {
                    thread::sleep(Duration::from_millis(50));
                }
            }
            Command::RunScript(script) => {
                println!("  {} Выполнение скрипта: {}", "▷".cyan(), script.yellow());
                self.simulate_script_execution()?;
            }
            Command::CheckDep(dep) => {
                print!("  {} Проверка зависимости: {} ... ", "?".blue(), dep.cyan());
                io::stdout().flush().unwrap();
                if !self.quick_mode {
                    thread::sleep(Duration::from_millis(200));
                }
                println!("{}", "OK".green().bold());
            }
            Command::WriteConfig { path, content } => {
                println!("  {} Запись конфигурации: {}", "📝".normal(), path.cyan());
                if self.verbose && !content.is_empty() {
                    for line in content.lines().take(3) {
                        println!("    {}", line.dimmed());
                    }
                    if content.lines().count() > 3 {
                        println!("    {}", "...".dimmed());
                    }
                }
                if !self.quick_mode {
                    thread::sleep(Duration::from_millis(100));
                }
            }
            Command::DetectCpu => { self.detect_cpu()?; }
            Command::DetectMemory => { self.detect_memory()?; }
            Command::DetectDisk => { self.detect_disk()?; }
            Command::DetectGpu => { self.detect_gpu()?; }
            Command::DetectNetwork => { self.detect_network()?; }
            Command::DetectOs => { self.detect_os()?; }
            Command::DetectKernel => { self.detect_kernel()?; }
            Command::DetectBios => { self.detect_bios()?; }
            Command::RunTest { name, duration } => { self.run_test(name, *duration)?; }
            Command::LoadKernelModule(module) => { self.load_kernel_module(module)?; }
            Command::UnloadKernelModule(module) => { self.unload_kernel_module(module)?; }
            Command::UpdateInitramfs => { self.update_initramfs()?; }
            Command::UpdateGrub => { self.update_grub()?; }
            Command::MountPartition { device, mount_point } => { self.mount_partition(device, mount_point)?; }
            Command::UnmountPartition(mount_point) => { self.unmount_partition(mount_point)?; }
            Command::FormatPartition { device, fs_type } => { self.format_partition(device, fs_type)?; }
            Command::CreatePartition { device, size } => { self.create_partition(device, size)?; }
            Command::SetHostname(hostname) => {
                println!("  {} Установка имени хоста: {}", "🖥".normal(), hostname.cyan());
                if self.verbose {
                    println!("    {} hostnamectl set-hostname {}", "$".dimmed(), hostname);
                }
                if !self.quick_mode { thread::sleep(Duration::from_millis(100)); }
            }
            Command::SetTimezone(tz) => {
                println!("  {} Установка часового пояса: {}", "🌍".normal(), tz.cyan());
                if self.verbose {
                    println!("    {} timedatectl set-timezone {}", "$".dimmed(), tz);
                }
                if !self.quick_mode { thread::sleep(Duration::from_millis(100)); }
            }
            Command::SetLocale(locale) => {
                println!("  {} Установка локали: {}", "🌐".normal(), locale.cyan());
                if self.verbose {
                    println!("    {} localectl set-locale LANG={}", "$".dimmed(), locale);
                }
                if !self.quick_mode { thread::sleep(Duration::from_millis(100)); }
            }
            Command::CreateUser { username, groups } => { self.create_user(username, groups)?; }
            Command::SetPassword(user) => {
                print!("  {} Установка пароля для {} ... ", "🔑".normal(), user.cyan());
                io::stdout().flush().unwrap();
                if !self.quick_mode { thread::sleep(Duration::from_millis(300)); }
                println!("{}", "OK".green());
            }
            Command::EnableService(service) => { self.manage_service(service, "enable")?; }
            Command::DisableService(service) => { self.manage_service(service, "disable")?; }
            Command::StartService(service) => { self.manage_service(service, "start")?; }
            Command::StopService(service) => { self.manage_service(service, "stop")?; }
            Command::InstallBootloader(target) => { self.install_bootloader(target)?; }
            Command::GenerateFstab => { self.generate_fstab()?; }
            Command::CheckIntegrity(target) => { self.check_integrity(target)?; }
            Command::VerifySignature(file) => { self.verify_signature(file)?; }
            Command::CompileKernel { version } => { self.compile_kernel(version)?; }
            Command::InstallPackages(packages) => { self.install_packages(packages)?; }
            Command::UpdateSystem => { self.update_system()?; }
            Command::SyncTime => { self.sync_time()?; }
            Command::TestHardware(component) => { self.test_hardware(component)?; }
            Command::BenchmarkCpu => { self.benchmark_cpu()?; }
            Command::BenchmarkMemory => { self.benchmark_memory()?; }
            Command::BenchmarkDisk => { self.benchmark_disk()?; }
            Command::NetworkConfig { interface, config } => { self.network_config(interface, config)?; }
            Command::FirewallRule(rule) => {
                println!("  {} Добавление правила firewall: {}", "🛡".normal(), rule.yellow());
                if !self.quick_mode { thread::sleep(Duration::from_millis(100)); }
            }
            Command::ScanHardware => { self.scan_hardware()?; }
            Command::DetectDrivers => { self.detect_drivers()?; }
            Command::InstallDriver(driver) => { self.install_driver(driver)?; }
        }
        Ok(())
    }

    // ===== Методы детекции =====

    fn detect_cpu(&mut self) -> Result<(), String> {
        print!("  {} Определение процессора ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(500)); }
        let (vendor, model, cores, freq) = RandomSystemInfo::cpu();
        println!();
        println!("    {} {} {}", "├".dimmed(), "Производитель:".dimmed(), vendor.cyan());
        println!("    {} {} {}", "├".dimmed(), "Модель:".dimmed(), model.white().bold());
        println!("    {} {} {} ядер", "├".dimmed(), "Ядра:".dimmed(), cores.to_string().yellow());
        println!("    {} {} {} MHz", "└".dimmed(), "Частота:".dimmed(), freq.to_string().green());
        Ok(())
    }

    fn detect_memory(&mut self) -> Result<(), String> {
        print!("  {} Определение памяти ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(400)); }
        let (size, mem_type, speed) = RandomSystemInfo::memory();
        println!();
        println!("    {} {} {} GB", "├".dimmed(), "Объём:".dimmed(), size.to_string().white().bold());
        println!("    {} {} {}", "├".dimmed(), "Тип:".dimmed(), mem_type.cyan());
        println!("    {} {} {} MHz", "└".dimmed(), "Скорость:".dimmed(), speed.to_string().green());
        Ok(())
    }

    fn detect_disk(&mut self) -> Result<(), String> {
        print!("  {} Определение накопителей ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(600)); }
        let (vendor, model, size, disk_type) = RandomSystemInfo::disk();
        println!();
        println!("    {} {} {}", "├".dimmed(), "Производитель:".dimmed(), vendor.cyan());
        println!("    {} {} {}", "├".dimmed(), "Модель:".dimmed(), model.white().bold());
        println!("    {} {} {} GB", "├".dimmed(), "Объём:".dimmed(), size.to_string().yellow());
        println!("    {} {} {}", "└".dimmed(), "Тип:".dimmed(), disk_type.green());
        Ok(())
    }

    fn detect_gpu(&mut self) -> Result<(), String> {
        print!("  {} Определение видеокарты ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(500)); }
        let (vendor, model, vram) = RandomSystemInfo::gpu();
        println!();
        println!("    {} {} {}", "├".dimmed(), "Производитель:".dimmed(), vendor.cyan());
        println!("    {} {} {}", "├".dimmed(), "Модель:".dimmed(), model.white().bold());
        println!("    {} {} {} GB VRAM", "└".dimmed(), "Память:".dimmed(), vram.to_string().green());
        Ok(())
    }

    fn detect_network(&mut self) -> Result<(), String> {
        print!("  {} Определение сетевых адаптеров ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(500)); }
        let (vendor, model, speed) = RandomSystemInfo::network();
        let mac = RandomSystemInfo::mac_address();
        let ip = RandomSystemInfo::ip_address();
        println!();
        println!("    {} {} {}", "├".dimmed(), "Адаптер:".dimmed(), format!("{} {}", vendor, model).white().bold());
        println!("    {} {} {}", "├".dimmed(), "Скорость:".dimmed(), speed.green());
        println!("    {} {} {}", "├".dimmed(), "MAC:".dimmed(), mac.yellow());
        println!("    {} {} {}", "└".dimmed(), "IP:".dimmed(), ip.cyan());
        Ok(())
    }

    fn detect_os(&mut self) -> Result<(), String> {
        print!("  {} Определение операционной системы ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(300)); }
        let (name, version) = RandomSystemInfo::os();
        println!();
        println!("    {} {} {}", "├".dimmed(), "Система:".dimmed(), name.white().bold());
        println!("    {} {} {}", "└".dimmed(), "Версия:".dimmed(), version.cyan());
        Ok(())
    }

    fn detect_kernel(&mut self) -> Result<(), String> {
        print!("  {} Определение версии ядра ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(200)); }
        let kernel = RandomSystemInfo::kernel();
        println!("{}", kernel.green());
        Ok(())
    }

    fn detect_bios(&mut self) -> Result<(), String> {
        print!("  {} Определение BIOS/UEFI ... ", "🔍".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(400)); }
        let (vendor, bios_type, version) = RandomSystemInfo::bios();
        println!();
        println!("    {} {} {}", "├".dimmed(), "Производитель:".dimmed(), vendor.cyan());
        println!("    {} {} {}", "├".dimmed(), "Тип:".dimmed(), bios_type.white().bold());
        println!("    {} {} {}", "└".dimmed(), "Версия:".dimmed(), version.green());
        Ok(())
    }

    fn run_test(&mut self, name: &str, duration: u64) -> Result<(), String> {
        print!("  {} Тест: {} ", "🧪".normal(), name.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode {
            let pb = ProgressBar::new(100);
            pb.set_style(ProgressStyle::default_bar()
                .template("[{bar:20.green/white}] {percent}%").unwrap()
                .progress_chars("█▓░"));
            let steps = 20;
            let step_duration = duration / steps;
            for i in 0..=steps { pb.set_position((i * 5) as u64); thread::sleep(Duration::from_millis(step_duration)); }
            pb.finish_and_clear();
        }
        println!("{}", "PASSED".green().bold());
        Ok(())
    }

    fn test_hardware(&mut self, component: &str) -> Result<(), String> {
        println!("  {} Тестирование {}", "🔬".normal(), component.cyan());
        let tests = match component {
            "memory" | "ram" => vec!["Проверка ячеек памяти", "Тест чтения/записи", "Стресс-тест"],
            "cpu" => vec!["Арифметические операции", "SIMD инструкции", "Температурный мониторинг"],
            "disk" | "storage" => vec!["Проверка секторов", "Тест SMART", "Скорость чтения/записи"],
            "gpu" => vec!["Рендеринг", "Вычисления CUDA/OpenCL", "Температура"],
            _ => vec!["Базовый тест", "Функциональный тест"],
        };
        for test in tests { self.run_test(test, 500)?; }
        Ok(())
    }

    fn benchmark_cpu(&mut self) -> Result<(), String> {
        println!("  {} CPU Benchmark", "📊".normal());
        if !self.quick_mode {
            let tests = [("Single-thread", "12,847"), ("Multi-thread", "98,432"), ("Floating point", "45,621"), ("Integer ops", "67,891")];
            for (name, score) in tests {
                print!("    {} {} ... ", "→".dimmed(), name);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(400));
                println!("{} points", score.green().bold());
            }
        } else {
            println!("    {} Score: {} points", "→".dimmed(), "98,432".green().bold());
        }
        Ok(())
    }

    fn benchmark_memory(&mut self) -> Result<(), String> {
        println!("  {} Memory Benchmark", "📊".normal());
        if !self.quick_mode {
            let tests = [("Read", "52,341 MB/s"), ("Write", "48,762 MB/s"), ("Copy", "45,123 MB/s"), ("Latency", "68.4 ns")];
            for (name, result) in tests {
                print!("    {} {} ... ", "→".dimmed(), name);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(300));
                println!("{}", result.green().bold());
            }
        }
        Ok(())
    }

    fn benchmark_disk(&mut self) -> Result<(), String> {
        println!("  {} Disk Benchmark", "📊".normal());
        if !self.quick_mode {
            let tests = [("Sequential Read", "3,521 MB/s"), ("Sequential Write", "3,012 MB/s"), ("Random Read 4K", "89,456 IOPS"), ("Random Write 4K", "76,234 IOPS")];
            for (name, result) in tests {
                print!("    {} {} ... ", "→".dimmed(), name);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(400));
                println!("{}", result.green().bold());
            }
        }
        Ok(())
    }

    fn load_kernel_module(&mut self, module: &str) -> Result<(), String> {
        print!("  {} Загрузка модуля ядра: {} ... ", "📦".normal(), module.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(300)); }
        if self.verbose { println!(); println!("    {} modprobe {}", "$".dimmed(), module); }
        println!("{}", "OK".green());
        Ok(())
    }

    fn unload_kernel_module(&mut self, module: &str) -> Result<(), String> {
        print!("  {} Выгрузка модуля ядра: {} ... ", "📤".normal(), module.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(200)); }
        println!("{}", "OK".green());
        Ok(())
    }

    fn update_initramfs(&mut self) -> Result<(), String> {
        println!("  {} Обновление initramfs...", "🔄".normal());
        if !self.quick_mode {
            let steps = ["Сборка модулей...", "Генерация образа...", "Сжатие (gzip)...", "Запись /boot/initramfs.img..."];
            for step in steps {
                print!("    {} {}", "→".dimmed(), step);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(400));
                println!(" {}", "✓".green());
            }
        }
        println!("    {} initramfs обновлён", "✓".green());
        Ok(())
    }

    fn update_grub(&mut self) -> Result<(), String> {
        println!("  {} Обновление GRUB...", "🔄".normal());
        if !self.quick_mode {
            let entries = ["Linux 6.6.8-arch1-1", "Linux 6.6.8-arch1-1 (fallback)", "Windows Boot Manager", "UEFI Firmware Settings"];
            println!("    {} Генерация grub.cfg...", "→".dimmed());
            thread::sleep(Duration::from_millis(300));
            println!("    {} Обнаруженные записи:", "→".dimmed());
            for entry in entries { thread::sleep(Duration::from_millis(150)); println!("      {} {}", "•".dimmed(), entry); }
        }
        println!("    {} GRUB обновлён", "✓".green());
        Ok(())
    }

    fn compile_kernel(&mut self, version: &str) -> Result<(), String> {
        println!("  {} Компиляция ядра {}", "🔨".normal(), version.cyan());
        if !self.quick_mode {
            let stages = [("Конфигурация", 500), ("Компиляция ядра", 2000), ("Компиляция модулей", 1500), ("Установка модулей", 800), ("Установка ядра", 400)];
            for (stage, duration) in stages {
                print!("    {} {} ", "→".dimmed(), stage);
                io::stdout().flush().unwrap();
                let pb = ProgressBar::new(100);
                pb.set_style(ProgressStyle::default_bar().template("[{bar:20.cyan/blue}]").unwrap().progress_chars("█▓░"));
                let steps = 20;
                for i in 0..=steps { pb.set_position((i * 5) as u64); thread::sleep(Duration::from_millis(duration / steps)); }
                pb.finish_and_clear();
                println!("{}", "✓".green());
            }
        }
        println!("    {} Ядро {} скомпилировано", "✓".green(), version);
        Ok(())
    }

    fn mount_partition(&mut self, device: &str, mount_point: &str) -> Result<(), String> {
        print!("  {} Монтирование {} → {} ... ", "💾".normal(), device.yellow(), mount_point.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(300)); }
        if self.verbose { println!(); println!("    {} mount {} {}", "$".dimmed(), device, mount_point); }
        println!("{}", "OK".green());
        Ok(())
    }

    fn unmount_partition(&mut self, mount_point: &str) -> Result<(), String> {
        print!("  {} Размонтирование {} ... ", "⏏".normal(), mount_point.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(200)); }
        println!("{}", "OK".green());
        Ok(())
    }

    fn format_partition(&mut self, device: &str, fs_type: &str) -> Result<(), String> {
        println!("  {} Форматирование {} в {}", "💿".normal(), device.yellow(), fs_type.cyan());
        if !self.quick_mode {
            print!("    {} Создание файловой системы ", "→".dimmed());
            io::stdout().flush().unwrap();
            let pb = ProgressBar::new(100);
            pb.set_style(ProgressStyle::default_bar().template("[{bar:30.yellow/white}] {percent}%").unwrap().progress_chars("█▓░"));
            for i in 0..=100 { pb.set_position(i); thread::sleep(Duration::from_millis(20)); }
            pb.finish_and_clear();
            println!("{}", "✓".green());
            if self.verbose { println!("    {} mkfs.{} {}", "$".dimmed(), fs_type, device); }
        }
        Ok(())
    }

    fn create_partition(&mut self, device: &str, size: &str) -> Result<(), String> {
        println!("  {} Создание раздела на {} ({})", "📀".normal(), device.yellow(), size.cyan());
        if !self.quick_mode {
            thread::sleep(Duration::from_millis(500));
            if self.verbose { println!("    {} parted {} mkpart primary 0% {}", "$".dimmed(), device, size); }
        }
        println!("    {} Раздел создан", "✓".green());
        Ok(())
    }

    fn generate_fstab(&mut self) -> Result<(), String> {
        println!("  {} Генерация /etc/fstab", "📝".normal());
        if !self.quick_mode {
            let entries = [("UUID=xxxx-xxxx", "/", "ext4", "defaults", "0 1"), ("UUID=yyyy-yyyy", "/boot/efi", "vfat", "umask=0077", "0 2"), ("UUID=zzzz-zzzz", "/home", "ext4", "defaults", "0 2"), ("tmpfs", "/tmp", "tmpfs", "defaults,nosuid,nodev", "0 0")];
            for (device, mount, fs, opts, dump) in entries {
                println!("    {} {} {} {} {} {}", "+".dimmed(), device.yellow(), mount.cyan(), fs, opts.dimmed(), dump.dimmed());
                thread::sleep(Duration::from_millis(150));
            }
        }
        println!("    {} fstab сгенерирован", "✓".green());
        Ok(())
    }

    fn create_user(&mut self, username: &str, groups: &str) -> Result<(), String> {
        println!("  {} Создание пользователя: {}", "👤".normal(), username.cyan());
        if !self.quick_mode { thread::sleep(Duration::from_millis(300)); }
        println!("    {} Группы: {}", "→".dimmed(), groups.yellow());
        if self.verbose { println!("    {} useradd -m -G {} {}", "$".dimmed(), groups, username); }
        println!("    {} Пользователь создан", "✓".green());
        Ok(())
    }

    fn manage_service(&mut self, service: &str, action: &str) -> Result<(), String> {
        let (icon, verb) = match action {
            "enable" => ("🔛", "Включение"), "disable" => ("🔚", "Отключение"),
            "start" => ("▶", "Запуск"), "stop" => ("⏹", "Остановка"), _ => ("⚙", "Управление"),
        };
        print!("  {} {} сервиса: {} ... ", icon, verb, service.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(200)); }
        if self.verbose { println!(); println!("    {} systemctl {} {}", "$".dimmed(), action, service); }
        println!("{}", "OK".green());
        Ok(())
    }

    fn install_bootloader(&mut self, target: &str) -> Result<(), String> {
        println!("  {} Установка загрузчика на {}", "🔧".normal(), target.yellow());
        if !self.quick_mode {
            let steps = ["Проверка EFI/BIOS режима...", "Установка загрузочных файлов...", "Создание записи в NVRAM...", "Генерация конфигурации..."];
            for step in steps {
                print!("    {} {}", "→".dimmed(), step);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(400));
                println!(" {}", "✓".green());
            }
        }
        println!("    {} GRUB установлен на {}", "✓".green(), target);
        Ok(())
    }

    fn check_integrity(&mut self, target: &str) -> Result<(), String> {
        println!("  {} Проверка целостности: {}", "🔍".normal(), target.cyan());
        if !self.quick_mode {
            print!("    {} Вычисление контрольных сумм ", "→".dimmed());
            io::stdout().flush().unwrap();
            let pb = ProgressBar::new(100);
            pb.set_style(ProgressStyle::default_bar().template("[{bar:25.cyan/white}]").unwrap().progress_chars("█▓░"));
            for i in 0..=100 { pb.set_position(i); thread::sleep(Duration::from_millis(15)); }
            pb.finish_and_clear();
            println!("{}", "OK".green());
        }
        println!("    {} Целостность подтверждена", "✓".green());
        Ok(())
    }

    fn verify_signature(&mut self, file: &str) -> Result<(), String> {
        print!("  {} Проверка подписи: {} ... ", "🔏".normal(), file.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(400)); }
        println!("{}", "VALID".green().bold());
        if self.verbose {
            let mut rng = rand::thread_rng();
            let key_id: u64 = rng.gen();
            println!("    {} Key ID: {:016X}", "→".dimmed(), key_id);
        }
        Ok(())
    }

    fn install_packages(&mut self, packages: &str) -> Result<(), String> {
        let pkg_list: Vec<&str> = packages.split_whitespace().collect();
        println!("  {} Установка пакетов ({} шт.)", "📦".normal(), pkg_list.len());
        if !self.quick_mode {
            for pkg in &pkg_list {
                print!("    {} {} ", "→".dimmed(), pkg.cyan());
                io::stdout().flush().unwrap();
                let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                for i in 0..10 {
                    print!("\r    {} {} {}", "→".dimmed(), pkg.cyan(), spinner_chars[i % spinner_chars.len()].to_string().cyan());
                    io::stdout().flush().unwrap();
                    thread::sleep(Duration::from_millis(80));
                }
                println!("\r    {} {} {}", "→".dimmed(), pkg.cyan(), "✓".green());
            }
        } else {
            for pkg in &pkg_list { println!("    {} {} {}", "→".dimmed(), pkg.cyan(), "✓".green()); }
        }
        Ok(())
    }

    fn update_system(&mut self) -> Result<(), String> {
        println!("  {} Обновление системы", "🔄".normal());
        if !self.quick_mode {
            let stages = ["Синхронизация репозиториев...", "Проверка обновлений...", "Загрузка пакетов...", "Установка обновлений...", "Очистка кэша..."];
            for stage in stages {
                print!("    {} {}", "→".dimmed(), stage);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(500));
                println!(" {}", "✓".green());
            }
        }
        let mut rng = rand::thread_rng();
        let updated = rng.gen_range(50..200);
        println!("    {} Обновлено {} пакетов", "✓".green(), updated);
        Ok(())
    }

    fn sync_time(&mut self) -> Result<(), String> {
        print!("  {} Синхронизация времени (NTP) ... ", "🕐".normal());
        io::stdout().flush().unwrap();
        if !self.quick_mode { thread::sleep(Duration::from_millis(500)); }
        println!("{}", "OK".green());
        if self.verbose {
            println!("    {} Сервер: pool.ntp.org", "→".dimmed());
            println!("    {} Смещение: +0.003s", "→".dimmed());
        }
        Ok(())
    }

    fn network_config(&mut self, interface: &str, config: &str) -> Result<(), String> {
        println!("  {} Настройка сети: {} ({})", "🌐".normal(), interface.cyan(), config.yellow());
        if !self.quick_mode {
            if config == "dhcp" {
                print!("    {} Получение IP через DHCP ", "→".dimmed());
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(800));
                let ip = RandomSystemInfo::ip_address();
                println!("{}", ip.green());
            } else {
                println!("    {} Применение статической конфигурации", "→".dimmed());
                thread::sleep(Duration::from_millis(300));
            }
            println!("    {} Проверка подключения...", "→".dimmed());
            thread::sleep(Duration::from_millis(400));
        }
        println!("    {} Сеть настроена", "✓".green());
        Ok(())
    }

    fn scan_hardware(&mut self) -> Result<(), String> {
        println!("  {} Сканирование оборудования", "🔎".normal());
        if !self.quick_mode {
            let devices = [("PCI", "Видеоадаптер, Сетевой контроллер, USB контроллер"), ("USB", "Клавиатура, Мышь, USB Hub"), ("ACPI", "Управление питанием, Термальные зоны"), ("SATA", "SSD, HDD"), ("NVMe", "NVMe SSD")];
            for (bus, found) in devices {
                print!("    {} Шина {} ... ", "→".dimmed(), bus.cyan());
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(300));
                println!("{}", found.dimmed());
            }
        }
        println!("    {} Сканирование завершено", "✓".green());
        Ok(())
    }

    fn detect_drivers(&mut self) -> Result<(), String> {
        println!("  {} Определение необходимых драйверов", "🔍".normal());
        if !self.quick_mode {
            let drivers = [("nvidia", "Видеокарта NVIDIA"), ("iwlwifi", "Intel Wi-Fi"), ("r8169", "Realtek Ethernet"), ("xhci_hcd", "USB 3.0"), ("nvme", "NVMe SSD"), ("snd_hda_intel", "Intel HD Audio")];
            for (drv, desc) in drivers {
                println!("    {} {} - {}", "+".dimmed(), drv.cyan(), desc.dimmed());
                thread::sleep(Duration::from_millis(150));
            }
        }
        Ok(())
    }

    fn install_driver(&mut self, driver: &str) -> Result<(), String> {
        print!("  {} Установка драйвера: {} ", "📦".normal(), driver.cyan());
        io::stdout().flush().unwrap();
        if !self.quick_mode {
            let spinner_chars = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
            for i in 0..15 {
                print!("\r  {} Установка драйвера: {} {}", "📦".normal(), driver.cyan(), spinner_chars[i % spinner_chars.len()].to_string().cyan());
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(100));
            }
        }
        println!("\r  {} Установка драйвера: {} {}", "📦".normal(), driver.cyan(), "✓".green());
        Ok(())
    }

    fn simulate_operation(&mut self, msg: &str, delay_ms: u64) -> Result<(), String> {
        print!("  {} {} ", "→".dimmed(), msg);
        io::stdout().flush().unwrap();
        if !self.quick_mode {
            let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let iterations = delay_ms / 80;
            for i in 0..iterations {
                print!("\r  {} {} {}", spinner_chars[i as usize % spinner_chars.len()].to_string().cyan(), msg, " ");
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(80));
            }
        }
        println!("\r  {} {} {}", "✓".green(), msg, " ");
        Ok(())
    }

    fn simulate_download(&mut self, url: &str, size: u64) -> Result<(), String> {
        println!("  {} Загрузка: {}", "⬇".blue(), url.cyan());
        if !self.quick_mode {
            let pb = ProgressBar::new(size);
            pb.set_style(ProgressStyle::default_bar().template("    [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap().progress_chars("█▉▊▋▌▍▎▏ "));
            let mut downloaded = 0u64;
            let mut rng = rand::thread_rng();
            while downloaded < size {
                let chunk = rng.gen_range(10..50).min((size - downloaded) as u64);
                downloaded += chunk;
                pb.set_position(downloaded);
                thread::sleep(Duration::from_millis(rng.gen_range(20..60)));
            }
            pb.finish_and_clear();
        }
        println!("    {} Загружено: {} байт", "✓".green(), size);
        Ok(())
    }

    fn simulate_extraction(&mut self, from: &str, to: &str) -> Result<(), String> {
        println!("  {} Распаковка: {} → {}", "📦".normal(), from.dimmed(), to.cyan());
        if !self.quick_mode {
            let files = vec!["bin/main", "lib/libcore.so", "share/data.dat", "etc/config.conf", "doc/README.md"];
            for file in files {
                print!("    {} {}", "→".dimmed(), file);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(100));
                println!(" {}", "✓".green());
            }
        } else {
            println!("    {} 5 файлов распаковано", "✓".green());
        }
        Ok(())
    }

    fn simulate_dep_install(&mut self, name: &str, version: &str) -> Result<(), String> {
        print!("  {} Установка зависимости: {} (v{}) ", "📦".normal(), name.cyan(), version.yellow());
        io::stdout().flush().unwrap();
        if !self.quick_mode {
            let spinner_chars = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
            for i in 0..15 {
                print!("\r  {} Установка зависимости: {} (v{}) {} ", "📦".normal(), name.cyan(), version.yellow(), spinner_chars[i % spinner_chars.len()].to_string().cyan());
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(100));
            }
        }
        println!("\r  {} Установка зависимости: {} (v{}) {}     ", "📦".normal(), name.cyan(), version.yellow(), "✓".green());
        Ok(())
    }

    fn simulate_script_execution(&mut self) -> Result<(), String> {
        if !self.quick_mode {
            let outputs = vec!["  Initializing...", "  Loading modules...", "  Applying configuration...", "  Done."];
            for output in outputs { println!("    {}", output.dimmed()); thread::sleep(Duration::from_millis(150)); }
        }
        Ok(())
    }

    fn show_progress_bar(&self, pct: u8) {
        let width = 30;
        let filled = (width * pct as usize) / 100;
        let empty = width - filled;
        let bar = format!("[{}{}] {}%", "█".repeat(filled).green(), "░".repeat(empty).dimmed(), pct);
        println!("  {} Прогресс: {}", "◉".blue(), bar);
    }
}

// ============== Main ==============

fn list_builtin() {
    println!();
    println!("{}", "Встроенные файлы установки:".green().bold());
    println!("{}", "─".repeat(40).dimmed());
    println!("  {}   - Установка Python 3.12", "python".cyan());
    println!("  {}   - Установка Node.js 20 LTS", "nodejs".cyan());
    println!("  {}   - Установка Docker Engine", "docker".cyan());
    println!("  {}    - Установка Arch Linux", "linux".cyan());
    println!("  {}      - Установка всего (Python + Node.js + Docker)", "all".cyan());
    println!("  {} - Полный стек разработчика", "devstack".cyan());
    println!();
    println!("Использование:");
    println!("  {} --file examples/<имя>.instnoth", "instnoth".yellow());
    println!("  {} --file examples/python.instnoth examples/nodejs.instnoth", "instnoth".yellow());
    println!("  {} --file examples/all.instnoth --show-deps", "instnoth".yellow());
    println!();
}

fn main() {
    let args = Args::parse();

    if args.list_builtin {
        list_builtin();
        return;
    }

    let files = match args.file {
        Some(f) => f,
        None => {
            eprintln!("{} Укажите файл(ы) установки: instnoth --file <путь.instnoth> [<путь2.instnoth> ...]", "✗".red());
            eprintln!("Используйте {} для просмотра встроенных файлов", "--list-builtin".cyan());
            std::process::exit(1);
        }
    };

    // Загружаем все указанные пакеты
    let mut packages = Vec::new();
    let mut base_path = PathBuf::from(".");

    for file_path in &files {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{} Не удалось прочитать файл {:?}: {}", "✗".red(), file_path, e);
                std::process::exit(1);
            }
        };

        // Устанавливаем базовый путь для зависимостей
        if let Some(parent) = file_path.parent() {
            base_path = parent.to_path_buf();
        }

        let mut parser = InstnothParser::with_path(content, file_path.clone());
        match parser.parse() {
            Ok(pkg) => packages.push(pkg),
            Err(e) => {
                eprintln!("{} Ошибка парсинга {:?}: {}", "✗".red(), file_path, e);
                std::process::exit(1);
            }
        }
    }

    let dep_manager = DependencyManager::new(base_path);

    // Показываем дерево зависимостей если запрошено
    if args.show_deps {
        println!();
        println!("{}", "Дерево зависимостей:".green().bold());
        println!("{}", "─".repeat(40).dimmed());
        let mut visited = HashSet::new();
        for pkg in &packages {
            show_dependency_tree(pkg, &dep_manager, 0, &mut visited);
        }
        println!();
        return;
    }

    // Определяем порядок установки с учётом зависимостей
    let install_order = if args.skip_deps {
        packages.clone()
    } else {
        let mut dm = DependencyManager::new(dep_manager.base_path.clone());
        match dm.get_install_order(&packages) {
            Ok(order) => order,
            Err(e) => {
                eprintln!("{} Ошибка разрешения зависимостей: {}", "✗".red(), e);
                std::process::exit(1);
            }
        }
    };

    // Выводим план установки
    if install_order.len() > 1 {
        println!();
        println!("{}", "╔═══════════════════════════════════════════════════════════════════╗".cyan());
        println!("{}", "║  InstNoth Multi-Package Installer                                 ║".cyan());
        println!("{}", "╚═══════════════════════════════════════════════════════════════════╝".cyan());
        println!();
        println!("{}: {} пакетов", "План установки".green().bold(), install_order.len());
        for (i, pkg) in install_order.iter().enumerate() {
            println!("  {}. {} (v{})", (i + 1).to_string().yellow(), pkg.name.cyan(), pkg.version);
        }
        println!();
        println!("{}", "───────────────────────────────────────────────────────────────────".dimmed());
    }

    // Запускаем установку каждого пакета
    let mut simulator = Simulator::new(args.quick, args.verbose);
    let mut installed_count = 0;

    for pkg in &install_order {
        if let Err(e) = simulator.run(pkg) {
            eprintln!("{} Ошибка установки {}: {}", "✗".red(), pkg.name, e);
            std::process::exit(1);
        }
        installed_count += 1;
    }

    // Финальное сообщение для множественной установки
    if install_order.len() > 1 {
        println!();
        println!("{}", "═══════════════════════════════════════════════════════════════════".green());
        println!("  {} Установлено {} пакетов:", "✓".green().bold(), installed_count);
        for pkg in &install_order {
            println!("    {} {} (v{})", "•".green(), pkg.name, pkg.version);
        }
        println!("{}", "═══════════════════════════════════════════════════════════════════".green());
        println!();
    }
}
