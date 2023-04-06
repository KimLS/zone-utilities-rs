use clap::{Parser, Subcommand};
use std::fs::{create_dir_all, read, read_dir, write};
use std::path::Path;
use zu_common::archive::prelude::*;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add or update files in the archive
    Add {
        #[clap(value_parser)]
        /// Path to the EverQuest archive to work with
        archive: String,

        #[clap(value_parser)]
        /// Files to add to the archive
        files: Vec<String>,
    },
    /// Delete files from the archive
    Delete {
        #[clap(value_parser)]
        /// Path to the EverQuest archive to work with
        archive: String,

        #[clap(value_parser)]
        /// Files to delete from the archive
        files: Vec<String>,
    },
    /// Extract files from the archive
    Extract {
        #[clap(value_parser)]
        /// Path to the EverQuest archive to work with
        archive: String,

        #[clap(short, long, value_parser)]
        /// Output directory to extract files to
        output_dir: Option<String>,

        #[clap(short, long, value_parser)]
        /// Files to extract from the archive
        files: Option<Vec<String>>,
    },
    /// List files in the archive
    List {
        #[clap(value_parser)]
        /// Path to the EverQuest archive to work with
        archive: String,

        #[clap(default_value_t = String::from(".*"), value_parser)]
        /// Regex to search for files by
        search_regex: String,
    },
    /// Pack all files in a directory into an archive
    Pack {
        #[clap(value_parser)]
        /// Path to the EverQuest archive to work with
        archive: String,

        #[clap(value_parser)]
        /// Input directory to pack files from
        input_dir: String,
    },
    /// Unpack all files in an archive into a directory
    Unpack {
        #[clap(value_parser)]
        /// Path to the EverQuest archive to work with
        archive: String,

        #[clap(value_parser)]
        /// Output directory to unpack files to
        output_dir: String,
    },
}

fn main() -> Result<(), ArchiveError> {
    let args = Cli::parse();

    match &args.command {
        Commands::Add { archive, files } => {
            add_to_archive(archive, files)?;
        }
        Commands::Delete { archive, files } => {
            delete_from_archive(archive, files)?;
        }
        Commands::Extract {
            archive,
            output_dir,
            files,
        } => {
            extract_from_archive(archive, output_dir, files)?;
        }
        Commands::List {
            archive,
            search_regex,
        } => {
            list_archive(archive, search_regex)?;
        }
        Commands::Pack { archive, input_dir } => {
            pack_directory(archive, input_dir)?;
        }
        Commands::Unpack {
            archive,
            output_dir,
        } => {
            unpack_to_directory(archive, output_dir)?;
        }
    }

    Ok(())
}

fn add_to_archive(filename: &str, files: &Vec<String>) -> Result<(), ArchiveError> {
    let mut archive = ReadWriteArchive::new();

    match archive.open_file(filename) {
        Ok(_) => println!("{} opened", filename),
        Err(_) => println!("creating a blank archive for {}", filename),
    }

    for file in files {
        let path = Path::new(file);
        let fname = path.file_name();

        if let Some(insert_fname) = fname {
            let name = insert_fname.to_str().unwrap();
            println!("adding {} to {}", name, filename);
            let data = read(file)?;
            archive.set(name, data)?;
        }
    }

    println!("saving...");
    archive.save_to_file(filename)?;
    println!("saved to {}", filename);
    Ok(())
}

fn delete_from_archive(filename: &str, files: &Vec<String>) -> Result<(), ArchiveError> {
    let mut archive = ReadWriteArchive::new();
    archive.open_file(filename)?;

    for file in files {
        archive.remove(file)?;
    }

    println!("saving...");
    archive.save_to_file(filename)?;
    println!("saved to {}", filename);
    Ok(())
}

fn extract_from_archive(
    filename: &str,
    output_dir: &Option<String>,
    files: &Option<Vec<String>>,
) -> Result<(), ArchiveError> {
    let mut archive = ReadableArchive::new();
    archive.open_file(filename)?;

    if let Some(output_dir) = output_dir {
        create_dir_all(output_dir)?;
    }

    if let Some(files) = files {
        extract_files(&archive, filename, output_dir, files);
    } else {
        let files = archive.search(".*")?;
        extract_files(&archive, filename, output_dir, &files);
    }

    Ok(())
}

fn extract_files(
    archive: &ReadableArchive,
    filename: &str,
    output_dir: &Option<String>,
    files: &Vec<String>,
) {
    for file in files {
        let data = match archive.get(file) {
            Ok(v) => v,
            Err(err) => {
                println!("unable to get {} in archive {}: {}", file, filename, err);
                continue;
            }
        };

        let path = get_path(file, output_dir);
        let len = data.len();
        match write(&path, data) {
            Ok(_) => println!("wrote {} bytes to {}", len, path),
            Err(err) => println!("unable to write {} to {}: {}", file, path, err),
        }
    }
}

fn get_path(filename: &str, output_dir: &Option<String>) -> String {
    if let Some(dir) = output_dir {
        format!("{}/{}", dir, filename)
    } else {
        filename.to_string()
    }
}

fn list_archive(filename: &str, search_regex: &str) -> Result<(), ArchiveError> {
    let mut archive = ReadableArchive::new();
    archive.open_file(filename)?;

    let files = archive.search(search_regex)?;
    println!("files in {} matching {}:", filename, search_regex);
    for file in &files {
        println!("{}", file);
    }

    Ok(())
}

fn pack_directory(filename: &str, input_dir: &String) -> Result<(), ArchiveError> {
    let mut archive = WritableArchive::new();
    let paths = read_dir(input_dir)?;

    for path in paths {
        let p = path?;

        match p.file_type() {
            Ok(ty) => {
                if ty.is_file() {
                    let data = read(p.path())?;
                    let osfname = p.file_name();
                    let fname = osfname.to_string_lossy();
                    archive.set(&fname, data)?;
                }
            }
            Err(err) => println!("error packing {}: {}", p.path().to_string_lossy(), err),
        }
    }

    archive.save_to_file(filename)?;

    Ok(())
}

fn unpack_to_directory(filename: &str, output_dir: &String) -> Result<(), ArchiveError> {
    let mut archive = ReadableArchive::new();
    archive.open_file(filename)?;

    create_dir_all(output_dir)?;

    let files = archive.search(".*")?;
    extract_files(&archive, filename, &Some(output_dir.to_string()), &files);

    Ok(())
}
