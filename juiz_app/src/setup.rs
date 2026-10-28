use juiz_core::prelude::*;
use std::fs::{create_dir, remove_dir_all, rename, File};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

use clap::Subcommand;

use crate::{default_juiz_conf::default_conf, Args};

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum SetupSubCommands {
    /// get logs
    #[clap(arg_required_else_help = false)]
    Home {},

    #[clap(arg_required_else_help = false)]
    Cleanhome {
        #[arg(
            short = 'f',
            help = "Force remove all. If not set, ${JUIZHOME}/conf/default.conf will be just renamed (not loaded by default.) "
        )]
        force: bool,
    },
}

pub(crate) fn on_setup(
    _manifest: Value,
    subcommand: SetupSubCommands,
    _args: Args,
) -> JuizResult<()> {
    match subcommand {
        SetupSubCommands::Home {} => on_setup_home(),
        SetupSubCommands::Cleanhome { force } => on_setup_cleanhome(force),
    }
}

fn on_setup_home() -> JuizResult<()> {
    match home::home_dir() {
        Some(homepath) => return create_juiz_home_dir(&homepath),
        None => println!("Impossible to get your home dir!"),
    }
    return Ok(());
}

fn on_setup_cleanhome(force: bool) -> JuizResult<()> {
    println!("cleanhome");
    match home::home_dir() {
        Some(homepath) => return remove_juiz_home_dir(&homepath, force),
        None => println!("Impossible to get your home dir!"),
    }
    println!("cleanhome");
    return Ok(());
}

fn create_juiz_home_dir(homepath: &Path) -> JuizResult<()> {
    let juiz_homepath = homepath.join(".juiz");
    if !juiz_homepath.exists() {
        create_dir(juiz_homepath.clone())?;
    }
    setup_juiz_homepath(juiz_homepath)
}

fn remove_juiz_home_dir(homepath: &Path, force: bool) -> JuizResult<()> {
    let juiz_homepath = homepath.join(".juiz");
    if !juiz_homepath.exists() {
        return Ok(());
    }
    if force {
        // -f はバックアップや置いたプラグインも含めてすべて消す
        // (remove_dir では中身が残っていると失敗していた)
        return Ok(remove_dir_all(juiz_homepath)?);
    }
    backup_default_conf(juiz_homepath)
}

fn setup_juiz_homepath(juiz_homepath: std::path::PathBuf) -> JuizResult<()> {
    let conf_path = juiz_homepath.join("conf");
    if !conf_path.exists() {
        create_dir(conf_path.clone())?;
    }
    let default_conf_filepath = conf_path.join("default.conf");
    if !default_conf_filepath.exists() {
        create_default_conf(default_conf_filepath)?;
    }

    let plugins_path = juiz_homepath.join("plugins");
    if !plugins_path.exists() {
        create_dir(plugins_path.clone())?;
    }
    let plugins_broker_factories_path = plugins_path.join("broker_factories");
    if !plugins_broker_factories_path.exists() {
        create_dir(plugins_broker_factories_path.clone())?;
    }

    Ok(())
}

fn create_default_conf(path: std::path::PathBuf) -> JuizResult<()> {
    let mut file = File::create(path)?;
    file.write(default_conf().as_bytes())?;
    Ok(())
}

/// ${JUIZHOME}/conf/default.conf を default.conf_backup (既にあれば _backup1, _backup2, ...) に改名し、
/// 既定では読み込まれないようにする
fn backup_default_conf(juiz_homepath: std::path::PathBuf) -> JuizResult<()> {
    let default_conf_filepath = juiz_homepath.join("conf").join("default.conf");
    if !default_conf_filepath.exists() {
        return Ok(());
    }
    let new_filename_base =
        PathBuf::from(default_conf_filepath.to_str().unwrap().to_owned() + "_backup");
    let mut new_filename = new_filename_base.clone();
    let mut counter: i64 = 1;
    // counter を増やしていなかったので、_backup1 まであると止まらなかった
    while new_filename.exists() {
        new_filename = PathBuf::from(
            new_filename_base.to_str().unwrap().to_owned() + counter.to_string().as_str(),
        );
        counter += 1;
    }
    rename(default_conf_filepath, new_filename)?;
    Ok(())
}
