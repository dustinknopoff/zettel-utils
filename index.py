from glob import glob

links = r"\[([^\[]+)\](\(.*\))"
headers = r"^(#{1,6})(.*)$"
tags = r"#[A-Za-z0-9-._]+"

directory = "/Users/dustinknopoff/Documents/1-Areas/Notes/wiki"

for zettel in glob(f"{directory}/**.md", recursive=True):
    print(zettel)
