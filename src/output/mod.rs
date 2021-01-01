mod alfred;
use serde_json::to_string_pretty;

use crate::arguments::OutFormat;
use crate::db::Zettel;

use self::alfred::AlfredResults;

pub fn execute(zettels: Vec<Zettel>, output_kind: &OutFormat) -> Result<(), anyhow::Error> {
    match output_kind {
        OutFormat::StdOut => {
            println!("{} results", zettels.len());
            println!("Title,Path");
            for zettel in zettels {
                println!("{},{}", zettel.title, zettel.file_path);
            }
        }
        OutFormat::JSON => println!("{}", to_string_pretty(&zettels)?),
        OutFormat::Alfred => {
            let out: AlfredResults = zettels.into();
            println!("{}", to_string_pretty(&out)?);
        }
    }
    Ok(())
}
