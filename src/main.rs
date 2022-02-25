// https://git-scm.com/docs/hash-function-transition/

use mmrbi::*;

use sha2::*;

use std::fs::File;
use std::io::{self, Write as _};
use std::path::*;



fn main() {
    create_demo_git();
}

fn create_demo_git() {
    let workspace = Path::new("");
    assert!(workspace.join("Cargo.lock").exists());

    let _ = std::fs::remove_dir_all("demo.git");
    let _ = std::fs::remove_dir_all("demo");

    let git             = workspace.join("demo.git");
    let git_objects     = git.join("objects");
    let git_refs        = git.join("refs");
    let git_refs_heads  = git_refs.join("heads");

    create_dir_all_or_panic(&git_refs_heads);

    mmrbi::fs::write_if_modified_with(git.join("config"), |c|{
        writeln!(c, "[core]")?;
        writeln!(c, "\trepositoryformatversion = 1")?;
        writeln!(c, "\tfilemode = false")?;
        writeln!(c, "\tbare = true")?;
        writeln!(c, "\tlogallrefupdates = true")?;
        writeln!(c, "\tsymlinks = false")?;
        writeln!(c, "\tignorecase = {:?}", cfg!(target_os="windows"))?;
        writeln!(c, "[extensions]")?;
        writeln!(c, "\tobjectFormat = sha256")?;
        Ok(())
    }).unwrap();

    let hello_world = create_blob_or_panic(&git_objects, &["Hello, world!\n".as_bytes()]);

    let root_tree = create_tree_or_panic(&git_objects, [
        // mode,    name,               hash
        ("100644",  "hello-world.txt",  &hello_world),
        // typical file mode: 100644, typical dir mode: 40000
    ]);

    let p = Person { name: "computer", email: "computer@example.com", date_seconds: "0", tz: Utc };
    let commit = create_commit_or_panic(&git_objects, &root_tree, [], [
        ("author",      &p),
        ("committer",   &p),
    ], "create_demo_git\n");

    mmrbi::fs::write_if_modified_with(git_refs_heads.join("master"), |r|{
        writeln!(r, "{commit}", commit = commit.hex)
    }).unwrap();

    mmrbi::fs::write_if_modified_with(git.join("HEAD"), |r|{
        writeln!(r, "ref: refs/heads/master")
    }).unwrap();

    Command::parse("git clone demo.git").unwrap().status0().unwrap();
}

fn create_dir_all_or_panic(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap_or_else(|e| fatal!("unable to create `{}`: {}", dir.display(), e))
}

fn create_file_or_panic(path: &Path, io: impl FnOnce(&mut File) -> io::Result<()>) {
    let mut f = File::create(path).unwrap_or_else(|e| fatal!("unable to create `{}`: {}", path.display(), e));
    io(&mut f).unwrap_or_else(|e| fatal!("error writing `{}`: {}", path.display(), e));
}

fn create_or_panic(ty: &str, git_objects: &Path, content: &[&[u8]]) -> Hash {
    assert!(git_objects.ends_with("objects"));

    let bytes  = content.iter().copied().map(|c| c.len()).sum::<usize>();
    let header = format!("{ty} {bytes}\0");

    let mut hash = Sha256::new();
    hash.update(header.as_bytes());
    for c in content.iter() { hash.update(c) }
    let hash = Hash::from(hash);

    let dir     = git_objects.join(&hash.hex[..2]);
    let file    = dir.join(&hash.hex[2..]);

    if file.exists() {
        // TODO: validate?
    } else {
        create_dir_all_or_panic(&dir);
        create_file_or_panic(&file, |f|{
            let mut f = flate2::write::ZlibEncoder::new(f, flate2::Compression::default());
            f.write_all(header.as_bytes())?;
            for c in content { f.write_all(&c[..])?; }
            f.try_finish()
        });
    }

    hash
}

fn create_blob_or_panic(git_objects: &Path, content: &[&[u8]]) -> Hash {
    create_or_panic("blob", git_objects, content)
}

fn create_tree_or_panic<'a>(git_objects: &Path, children: impl IntoIterator<Item = (&'a str, &'a str, &'a Hash)>) -> Hash {
    assert!(git_objects.ends_with("objects"));
    let mut fragments = Vec::<&[u8]>::new();

    for (mode, name, hash) in children {
        assert!(!mode.contains(" "));
        assert!(!mode.contains("\0"));
        assert!(!name.contains("\0"));
        fragments.push(mode.as_bytes());
        fragments.push(b" ");
        fragments.push(name.as_bytes());
        fragments.push(b"\0");
        fragments.push(hash.value.as_ref());
    }
    create_or_panic("tree", &git_objects, &fragments)
}

fn create_commit_or_panic<'a>(git_objects: &Path, tree: &Hash, parents: impl IntoIterator<Item = &'a Hash>, contribs: impl IntoIterator<Item = (&'a str, &'a Person<'a>)>, message: &str) -> Hash {
    assert!(git_objects.ends_with("objects"));
    let mut fragments = Vec::<&[u8]>::new();

    fragments.push(b"tree ");
    fragments.push(tree.hex.as_bytes());
    fragments.push(b"\n");

    for parent in parents {
        fragments.push(b"parent ");
        fragments.push(parent.hex.as_bytes());
        fragments.push(b"\n");
    }

    for (role, person) in contribs {
        fragments.push(role.as_bytes());
        fragments.push(b" ");
        fragments.push(person.name.as_bytes());
        fragments.push(b" <");
        fragments.push(person.email.as_bytes());
        fragments.push(b"> ");
        fragments.push(person.date_seconds.as_bytes());
        fragments.push(b" +0000\n"); let _ : Utc = person.tz;
    }
    fragments.push(b"\n");
    fragments.push(message.as_bytes());

    create_or_panic("commit", &git_objects, &fragments)
}

fn hex(hash: &impl AsRef<[u8]>) -> String {
    hash.as_ref().iter().copied().map(|b|{
        let b = b as usize;
        let hex = b"0123456789abcdef";
        let hi  = hex[b>>4] as char;
        let lo  = hex[b&15] as char;
        [hi, lo]
    }).flatten().collect::<String>()
}

struct Hash {
    pub value:  [u8; 32],
    pub hex:    String,
}

impl From<Sha256> for Hash {
    fn from(hash: Sha256) -> Self {
        let value : [u8; 32] = hash.finalize().into();
        let hex = hex(&value);
        Self { value, hex }
    }
}

impl std::fmt::Debug for Hash {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.write_str(&self.hex)
    }
}

#[derive(Clone, Copy, Debug)]
struct Person<'a> {
    name:           &'a str,
    email:          &'a str,
    date_seconds:   &'a str, // u32 would result in y2106 bug
    tz:             Utc,
}

#[derive(Clone, Copy, Debug)]
struct Utc;
