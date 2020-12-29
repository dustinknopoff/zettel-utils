use serde::{Deserialize, Serialize};

use crate::db::Zettel;

#[derive(Serialize, Deserialize, Debug)]
pub struct AlfredResults {
    items: Vec<Item>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    uid: Option<String>,
    #[serde(rename = "type")]
    item_type: String,
    title: String,
    subtitle: Option<String>,
    arg: Option<String>,
    autocomplete: Option<String>,
    icon: Option<Icon>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Icon {
    #[serde(rename = "type")]
    icon_type: String,
    path: String,
}

impl From<Vec<Zettel>> for AlfredResults {
    fn from(src: Vec<Zettel>) -> Self {
        let items = src
            .into_iter()
            .map(|zettel| Item {
                uid: Some(zettel.zettel_id),
                item_type: String::from("file"),
                title: zettel.title.clone(),
                subtitle: Some(zettel.file_path.clone()),
                arg: Some(zettel.file_path.clone()),
                autocomplete: Some(zettel.title),
                icon: Some(Icon {
                    icon_type: String::from("filetype"),
                    path: zettel.file_path,
                }),
            })
            .collect();
        Self { items }
    }
}
