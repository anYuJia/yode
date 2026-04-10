use std::path::PathBuf;

use anyhow::Result;

use crate::{check_workspace_package_versions, Cli, Commands, ProviderAction, UpdateAction};

pub(crate) async fn handle_cli_command(command: Commands, config: &mut yode_core::config::Config) -> Result<()> {
    match command {
        Commands::Provider { action } => {
            match action {
                ProviderAction::Add => {
                    yode_core::setup::run_setup_interactive()?;
                }
                ProviderAction::List => {
                    println!("已配置的提供商列表:");
                    for (name, p) in &config.llm.providers {
                        let is_default = if name == &config.llm.default_provider {
                            " (当前默认)"
                        } else {
                            ""
                        };
                        println!(
                            "- {}{} [格式: {}, Base URL: {}]",
                            name,
                            is_default,
                            p.format,
                            p.base_url.as_deref().unwrap_or("")
                        );
                    }
                }
                ProviderAction::Remove { name } => {
                    if config.llm.providers.remove(&name).is_some() {
                        config.save()?;
                        println!("已删除提供商: {}", name);
                    } else {
                        println!("未找到名为 '{}' 的提供商", name);
                    }
                }
                ProviderAction::SetDefault { name } => {
                    if config.llm.providers.contains_key(&name) {
                        config.llm.default_provider = name.clone();
                        config.save()?;
                        println!("已将 '{}' 设置为默认提供商", name);
                    } else {
                        println!("未找到名为 '{}' 的提供商", name);
                    }
                }
            }
            Ok(())
        }
        Commands::Update { action } => {
            let action = action.unwrap_or(UpdateAction::Check);
            match action {
                UpdateAction::Check => {
                    println!("正在检查更新...");
                    let config_dir = dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join(".yode");
                    let updater = yode_core::updater::Updater::new(config_dir, true, true);
                    match updater.check_for_updates().await {
                        Ok(Some(result)) => {
                            println!("✨ 发现新版本: {}", result.latest_version);
                            println!("   当前版本: {}", yode_core::updater::CURRENT_VERSION);
                            println!("\n发布日志:\n{}", result.release_notes);
                            println!("\n正在下载并安装更新...");
                            match updater.download_update(&result).await {
                                Ok(_) => match updater.apply_downloaded_update() {
                                    Ok(true) => {
                                        println!("✓ 更新已安装完成。新版本: {}", result.latest_version);
                                    }
                                    Ok(false) => {
                                        println!(
                                            "✗ 更新已下载，但未能自动应用。请重新运行 yode，或手动执行安装脚本。"
                                        );
                                    }
                                    Err(e) => {
                                        println!("✗ 更新已下载，但自动应用失败: {}", e);
                                    }
                                },
                                Err(e) => {
                                    println!("✗ 下载失败: {}", e);
                                    println!("\n你可以手动更新:");
                                    println!("  curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash");
                                }
                            }
                        }
                        Ok(None) => {
                            println!(
                                "✓ 当前已是最新版本 ({})",
                                yode_core::updater::CURRENT_VERSION
                            );
                        }
                        Err(e) => {
                            println!("✗ 检查更新失败: {}", e);
                        }
                    }
                }
                UpdateAction::Status => {
                    println!("更新配置状态:");
                    println!("  自动检查: {}", config.update.auto_check);
                    println!("  自动下载: {}", config.update.auto_download);
                    println!("  当前版本: {}", yode_core::updater::CURRENT_VERSION);
                    match yode_core::updater::latest_local_release_tag() {
                        Some(tag) => {
                            let status = if yode_core::updater::release_version_matches_tag(
                                &tag,
                                yode_core::updater::CURRENT_VERSION,
                            ) {
                                "匹配"
                            } else {
                                "不匹配"
                            };
                            println!("  最新本地 tag: {} ({})", tag, status);
                        }
                        None => {
                            println!("  最新本地 tag: 未找到");
                        }
                    }
                }
                UpdateAction::Preflight => {
                    println!("正在运行发布前检查...");
                    let mut has_failure = false;

                    let git_status = std::process::Command::new("git")
                        .args(["status", "--porcelain"])
                        .output();
                    match git_status {
                        Ok(output) if output.status.success() => {
                            let dirty = !String::from_utf8_lossy(&output.stdout).trim().is_empty();
                            if dirty {
                                has_failure = true;
                                println!("  [!!] 工作树不干净，请先提交或清理改动");
                            } else {
                                println!("  [ok] 工作树干净");
                            }
                        }
                        _ => {
                            has_failure = true;
                            println!("  [!!] 无法检查 git 工作树状态");
                        }
                    }

                    match yode_core::updater::latest_local_release_tag() {
                        Some(tag)
                            if yode_core::updater::release_version_matches_tag(
                                &tag,
                                yode_core::updater::CURRENT_VERSION,
                            ) =>
                        {
                            println!(
                                "  [ok] 版本与最新 tag 一致: {} == {}",
                                yode_core::updater::CURRENT_VERSION,
                                tag
                            );
                        }
                        Some(tag) => {
                            has_failure = true;
                            println!(
                                "  [!!] 版本与最新 tag 不一致: Cargo={} latest-tag={}",
                                yode_core::updater::CURRENT_VERSION,
                                tag
                            );
                        }
                        None => {
                            println!("  [--] 未找到本地 release tag，跳过版本对比");
                        }
                    }

                    match check_workspace_package_versions() {
                        Ok(()) => println!("  [ok] workspace package versions consistent"),
                        Err(err) => {
                            has_failure = true;
                            println!("  [!!] workspace package version check failed: {}", err);
                        }
                    }

                    for (label, mut command) in [
                        (
                            "cargo check",
                            {
                                let mut cmd = std::process::Command::new("cargo");
                                cmd.arg("check");
                                cmd
                            },
                        ),
                        (
                            "cargo test -p yode-tools",
                            {
                                let mut cmd = std::process::Command::new("cargo");
                                cmd.args(["test", "-p", "yode-tools"]);
                                cmd
                            },
                        ),
                    ] {
                        match command.status() {
                            Ok(status) if status.success() => println!("  [ok] {}", label),
                            Ok(_) => {
                                has_failure = true;
                                println!("  [!!] {} 失败", label);
                            }
                            Err(err) => {
                                has_failure = true;
                                println!("  [!!] 无法运行 {}: {}", label, err);
                            }
                        }
                    }

                    if has_failure {
                        anyhow::bail!("发布前检查失败");
                    }

                    println!("  [ok] 发布前检查通过");
                }
                UpdateAction::Notes { from, limit } => {
                    let base = from
                        .or_else(yode_core::updater::latest_local_release_tag)
                        .unwrap_or_else(|| "HEAD~20".to_string());
                    let range = format!("{}..HEAD", base);
                    let output = std::process::Command::new("git")
                        .args([
                            "log",
                            "--pretty=format:- %s",
                            "--no-merges",
                            &format!("--max-count={}", limit),
                            &range,
                        ])
                        .output();
                    match output {
                        Ok(output) if output.status.success() => {
                            let notes = String::from_utf8_lossy(&output.stdout);
                            println!("# Release notes draft\n");
                            println!("Range: {}\n", range);
                            if notes.trim().is_empty() {
                                println!("No commits found.");
                            } else {
                                println!("{}", notes);
                            }
                        }
                        Ok(output) => {
                            anyhow::bail!(
                                "failed to generate release notes: {}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                        }
                        Err(err) => {
                            anyhow::bail!("failed to run git log: {}", err);
                        }
                    }
                }
            }
            Ok(())
        }
        Commands::Completions { shell } => {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            let bin_name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
            Ok(())
        }
        Commands::Doctor => {
            println!("正在进行环境健康检查...");
            let git_v = std::process::Command::new("git").arg("--version").output();
            match git_v {
                Ok(o) if o.status.success() => {
                    println!(
                        "  [ok] git available: {}",
                        String::from_utf8_lossy(&o.stdout).trim()
                    );
                }
                _ => println!("  [!!] git not found"),
            }

            for runtime in ["node", "python3", "go", "cargo"] {
                let out = std::process::Command::new(runtime)
                    .arg("--version")
                    .output();
                if let Ok(o) = out {
                    println!(
                        "  [ok] {} available: {}",
                        runtime,
                        String::from_utf8_lossy(&o.stdout).trim()
                    );
                } else {
                    println!("  [--] {} not found", runtime);
                }
            }

            if config.llm.providers.is_empty() {
                println!("  [!!] No LLM providers configured.");
            } else {
                println!("  [ok] {} providers configured", config.llm.providers.len());
            }

            match yode_core::updater::latest_local_release_tag() {
                Some(tag)
                    if yode_core::updater::release_version_matches_tag(
                        &tag,
                        yode_core::updater::CURRENT_VERSION,
                    ) =>
                {
                    println!(
                        "  [ok] Version matches latest local tag: {} == {}",
                        yode_core::updater::CURRENT_VERSION,
                        tag
                    );
                }
                Some(tag) => {
                    println!(
                        "  [!!] Version/tag mismatch: Cargo={} latest-tag={}",
                        yode_core::updater::CURRENT_VERSION,
                        tag
                    );
                }
                None => {
                    println!("  [--] Could not determine latest local release tag");
                }
            }

            println!(
                "\nPlatform: {} {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            println!("Version:  v{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
