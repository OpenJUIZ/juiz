use juiz_core::prelude::*;
use std::fs::{create_dir, remove_dir, remove_file, rename, File};
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
    if juiz_homepath.exists() {
        cleanup_juiz_homepath(juiz_homepath.clone(), force)?;
    }
    if !force {
        return Ok(());
    }
    Ok(remove_dir(juiz_homepath)?)
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

fn cleanup_juiz_homepath(juiz_homepath: std::path::PathBuf, force: bool) -> JuizResult<()> {
    let conf_path = juiz_homepath.join("conf");
    if conf_path.exists() {
        let default_conf_filepath = conf_path.join("default.conf");
        if default_conf_filepath.exists() {
            if force {
                remove_file(default_conf_filepath)?;
            } else {
                let new_filename_base =
                    PathBuf::from(default_conf_filepath.to_str().unwrap().to_owned() + "_backup");
                let mut new_filename = new_filename_base.clone();
                let counter: i64 = 1;
                while new_filename.exists() {
                    new_filename = PathBuf::from(
                        new_filename_base.to_str().unwrap().to_owned()
                            + counter.to_string().as_str(),
                    );
                }
                rename(default_conf_filepath, new_filename)?;
                return Ok(());
            }
        } else {
            if !force {
                return Ok(());
            }
        }
        remove_dir(conf_path.clone())?;
    }

    let plugins_path = juiz_homepath.join("plugins");
    if plugins_path.exists() {
        let plugins_broker_factories_path = plugins_path.join("broker_factories");
        if plugins_broker_factories_path.exists() {
            remove_dir(plugins_broker_factories_path.clone())?;
        }
        remove_dir(plugins_path.clone())?;
    }

    Ok(())
}
