// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Petname Generator & Fuzzy Search
//
// Generates deterministic "adjective-animal" names and
// provides nucleo-based fuzzy matching over them.
// =========================================================

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use torchsnap_plugin_sdk::{Action, ActionId, EntryIcon, ScoredEntry};

// =========================================================
// Word Lists
// =========================================================

const ADJECTIVES: &[&str] = &[
    "able", "above", "absolute", "accepted", "accurate", "ace", "active",
    "actual", "adapted", "adequate", "advanced", "alert", "alive", "amazing",
    "ample", "amused", "apparent", "apt", "artistic", "assured", "awaited",
    "awake", "aware", "balanced", "becoming", "beloved", "better", "big",
    "blessed", "bold", "boss", "brave", "brief", "bright", "busy", "calm",
    "capable", "capital", "careful", "caring", "casual", "central", "certain",
    "champion", "charmed", "charming", "cheerful", "chief", "choice", "civil",
    "classic", "clean", "clear", "clever", "climbing", "close", "coherent",
    "comic", "communal", "complete", "composed", "concise", "concrete",
    "content", "cool", "correct", "cosmic", "crack", "creative", "credible",
    "crisp", "crucial", "cuddly", "cunning", "curious", "current", "cute",
    "daring", "darling", "dashing", "dear", "decent", "deep", "definite",
    "delicate", "desired", "destined", "devoted", "direct", "discrete",
    "distinct", "diverse", "divine", "dominant", "driven", "dynamic", "eager",
    "easy", "electric", "elegant", "emerging", "eminent", "enabled", "endless",
    "engaged", "enhanced", "enormous", "enough", "epic", "equal", "equipped",
    "eternal", "ethical", "evident", "evolved", "exact", "excited", "exciting",
    "exotic", "expert", "factual", "fair", "faithful", "famous", "fancy",
    "fast", "feasible", "fine", "finer", "firm", "first", "fit", "fitting",
    "fleet", "flexible", "flowing", "fluent", "flying", "fond", "frank",
    "free", "fresh", "full", "fun", "funky", "funny", "generous", "gentle",
    "genuine", "giving", "glad", "glorious", "glowing", "golden", "good",
    "gorgeous", "grand", "grateful", "great", "growing", "grown", "guided",
    "handy", "happy", "hardy", "harmless", "healthy", "helpful", "heroic",
    "hip", "holy", "honest", "hopeful", "hot", "huge", "humane", "humble",
    "humorous", "ideal", "immense", "immortal", "immune", "improved",
    "infinite", "informed", "innocent", "inspired", "integral", "intense",
    "intent", "intimate", "inviting", "joint", "just", "keen", "key", "kind",
    "knowing", "known", "large", "lasting", "leading", "learning", "legal",
    "lenient", "liberal", "light", "liked", "live", "living", "logical",
    "loved", "loving", "loyal", "lucky", "magical", "magnetic", "main",
    "major", "many", "massive", "master", "mature", "maximum", "measured",
    "merry", "mighty", "mint", "model", "modern", "modest", "moral", "more",
    "moved", "moving", "musical", "mutual", "national", "native", "natural",
    "nearby", "neat", "needed", "neutral", "new", "next", "nice", "noble",
    "normal", "notable", "noted", "novel", "obliging", "one", "open",
    "optimal", "organic", "patient", "peaceful", "perfect", "pet", "picked",
    "pleasant", "pleased", "pleasing", "poetic", "polished", "polite",
    "popular", "positive", "possible", "powerful", "precious", "precise",
    "premium", "prepared", "present", "pretty", "primary", "prime", "pro",
    "probable", "profound", "promoted", "prompt", "proper", "proud", "proven",
    "pumped", "pure", "quality", "quick", "quiet", "rapid", "rare",
    "rational", "ready", "real", "refined", "regular", "related", "relative",
    "relaxed", "relevant", "relieved", "renewed", "resolved", "rested",
    "rich", "right", "robust", "romantic", "ruling", "sacred", "safe",
    "saved", "saving", "secure", "select", "sensible", "set", "settled",
    "sharing", "sharp", "shining", "simple", "sincere", "singular", "skilled",
    "smart", "smashing", "smiling", "smooth", "social", "solid", "sought",
    "sound", "special", "splendid", "square", "stable", "star", "steady",
    "sterling", "still", "stirred", "striking", "strong", "stunning",
    "subtle", "suitable", "suited", "summary", "sunny", "super", "superb",
    "supreme", "sure", "sweeping", "sweet", "talented", "tender", "thankful",
    "thorough", "tidy", "tight", "together", "tolerant", "top", "topical",
    "tops", "touched", "touching", "tough", "true", "trusted", "trusting",
    "trusty", "ultimate", "unbiased", "uncommon", "unified", "unique",
    "united", "upright", "upward", "usable", "useful", "valid", "valued",
    "vast", "verified", "viable", "vital", "vocal", "wanted", "warm",
    "wealthy", "welcome", "well", "whole", "willing", "winning", "wired",
    "wise", "witty", "wondrous", "workable", "working", "worthy",
];

const NAMES: &[&str] = &[
    "ox", "ant", "ape", "asp", "bat", "bee", "boa", "bug", "cat", "cod",
    "cow", "cub", "doe", "dog", "eel", "eft", "elf", "elk", "emu", "ewe",
    "fly", "fox", "gar", "gnu", "hen", "hog", "imp", "jay", "kid", "kit",
    "koi", "lab", "man", "owl", "pig", "pug", "pup", "ram", "rat", "ray",
    "yak", "bass", "bear", "bird", "boar", "buck", "bull", "calf", "chow",
    "clam", "colt", "crab", "crow", "dane", "deer", "dodo", "dory", "dove",
    "drum", "duck", "fawn", "fish", "flea", "foal", "fowl", "frog", "gnat",
    "goat", "grub", "gull", "hare", "hawk", "ibex", "joey", "kite", "kiwi",
    "lamb", "lark", "lion", "loon", "lynx", "mako", "mink", "mite", "mole",
    "moth", "mule", "mutt", "newt", "orca", "oryx", "pika", "pony", "puma",
    "seal", "shad", "slug", "sole", "stag", "stud", "swan", "tahr", "teal",
    "tick", "toad", "tuna", "wasp", "wolf", "worm", "wren", "yeti", "adder",
    "akita", "alien", "aphid", "bison", "boxer", "bream", "bunny", "burro",
    "camel", "chimp", "civet", "cobra", "coral", "corgi", "crane", "dingo",
    "drake", "eagle", "egret", "filly", "finch", "gator", "gecko", "ghost",
    "ghoul", "goose", "guppy", "heron", "hippo", "horse", "hound", "husky",
    "hyena", "koala", "krill", "leech", "lemur", "liger", "llama", "louse",
    "macaw", "midge", "molly", "moose", "moray", "mouse", "panda", "perch",
    "prawn", "quail", "racer", "raven", "rhino", "robin", "satyr", "shark",
    "sheep", "shrew", "skink", "skunk", "sloth", "snail", "snake", "snipe",
    "squid", "stork", "swift", "swine", "tapir", "tetra", "tiger", "troll",
    "trout", "viper", "wahoo", "whale", "zebra", "alpaca", "amoeba",
    "baboon", "badger", "beagle", "bedbug", "beetle", "bengal", "bobcat",
    "caiman", "cattle", "cicada", "collie", "condor", "cougar", "coyote",
    "dassie", "donkey", "dragon", "earwig", "falcon", "feline", "ferret",
    "gannet", "gibbon", "glider", "goblin", "gopher", "grouse", "guinea",
    "hermit", "hornet", "iguana", "impala", "insect", "jackal", "jaguar",
    "jennet", "kitten", "kodiak", "lizard", "locust", "maggot", "magpie",
    "mammal", "mantis", "marlin", "marmot", "marten", "martin", "mayfly",
    "minnow", "monkey", "mullet", "muskox", "ocelot", "oriole", "osprey",
    "oyster", "parrot", "pigeon", "piglet", "poodle", "possum", "python",
    "quagga", "rabbit", "raptor", "rodent", "roughy", "salmon", "sawfly",
    "serval", "shiner", "shrimp", "spider", "sponge", "tarpon", "thrush",
    "tomcat", "toucan", "turkey", "turtle", "urchin", "vervet", "walrus",
    "weasel", "weevil", "wombat", "anchovy", "anemone", "bluejay", "buffalo",
    "bulldog", "buzzard", "caribou", "catfish", "chamois", "cheetah",
    "chicken", "chigger", "cowbird", "crappie", "crawdad", "cricket",
    "dogfish", "dolphin", "firefly", "garfish", "gazelle", "gelding",
    "giraffe", "gobbler", "gorilla", "goshawk", "grackle", "griffon",
    "grizzly", "grouper", "haddock", "hagfish", "halibut", "hamster",
    "herring", "jackass", "javelin", "jawfish", "jaybird", "katydid",
    "ladybug", "lamprey", "lemming", "leopard", "lioness", "lobster",
    "macaque", "mallard", "mammoth", "manatee", "mastiff", "meerkat",
    "mollusk", "monarch", "mongrel", "monitor", "monster", "mudfish",
    "muskrat", "mustang", "narwhal", "oarfish", "octopus", "opossum",
    "ostrich", "panther", "peacock", "pegasus", "pelican", "penguin",
    "phoenix", "piranha", "polecat", "primate", "quetzal", "raccoon",
    "rattler", "redbird", "redfish", "reptile", "rooster", "sawfish",
    "sculpin", "seagull", "skylark", "snapper", "spaniel", "sparrow",
    "sunbeam", "sunbird", "sunfish", "tadpole", "termite", "terrier",
    "unicorn", "vulture", "wallaby", "walleye", "warthog", "whippet",
    "wildcat", "aardvark", "airedale", "albacore", "anteater", "antelope",
    "arachnid", "barnacle", "basilisk", "blowfish", "bluebird", "bluegill",
    "bonefish", "bullfrog", "cardinal", "chipmunk", "cockatoo", "crayfish",
    "dinosaur", "doberman", "duckling", "elephant", "escargot", "flamingo",
    "flounder", "foxhound", "glowworm", "goldfish", "grubworm", "hedgehog",
    "honeybee", "hookworm", "humpback", "kangaroo", "killdeer", "kingfish",
    "labrador", "lacewing", "ladybird", "lionfish", "longhorn", "mackerel",
    "malamute", "marmoset", "mastodon", "moccasin", "mongoose", "monkfish",
    "mosquito", "pangolin", "parakeet", "pheasant", "pipefish", "platypus",
    "polliwog", "porpoise", "reindeer", "ringtail", "sailfish", "scorpion",
    "seahorse", "seasnail", "sheepdog", "shepherd", "silkworm", "squirrel",
    "stallion", "starfish", "starling", "stingray", "stinkbug", "sturgeon",
    "terrapin", "titmouse", "tortoise", "treefrog", "werewolf", "woodcock",
];

// =========================================================
// Petname Generation
//
// Generates deterministic names by iterating through all
// adjective-animal combinations. Wraps around if count
// exceeds the total combinations.
// =========================================================

pub fn generate_petnames(count: usize) -> Vec<String> {
    let mut names = Vec::with_capacity(count);
    let total_combinations = ADJECTIVES.len() * NAMES.len();

    for i in 0..count {
        let idx = i % total_combinations;
        let adj_idx = idx / NAMES.len();
        let name_idx = idx % NAMES.len();
        names.push(format!("{}-{}", ADJECTIVES[adj_idx], NAMES[name_idx]));
    }

    names
}

// =========================================================
// Fuzzy Search
//
// Uses nucleo-matcher (same engine as the host) to fuzzy
// match a query against the petname list. Returns scored
// entries with highlight positions.
// =========================================================

pub fn fuzzy_search(query: &str, names: &[String]) -> Vec<ScoredEntry> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Atom::new(query, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy, false);

    let mut buf = Vec::new();
    let mut results: Vec<ScoredEntry> = Vec::new();

    for name in names {
        let haystack = Utf32Str::new(name, &mut buf);
        let mut indices = Vec::new();

        if let Some(score) = pattern.indices(haystack, &mut matcher, &mut indices) {
            // nucleo returns UTF-32 indices — convert to UTF-16
            // offsets for the frontend. For ASCII petnames these
            // are identical, but we do it correctly regardless.
            let utf16_positions: Vec<u32> = indices
                .iter()
                .map(|&idx| char_index_to_utf16_offset(name, idx as usize))
                .collect();

            results.push(ScoredEntry {
                id: name.clone(),
                title: name.clone(),
                subtitle: Some("Petname (WASM fuzzy search)".into()),
                icon: Some(EntryIcon::Emoji("🐾".into())),
                score: score as u32,
                title_highlight_positions: utf16_positions,
                subtitle_highlight_positions: vec![],
                actions: vec![Action {
                    id: ActionId::Copy,
                    label: "Copy".into(),
                }],
            });
        }

        buf.clear();
    }

    // Sort by score descending, take top 20.
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.truncate(20);
    results
}

/// Convert a character index (0-based) to a UTF-16 code unit
/// offset. For ASCII strings these are identical, but this
/// handles multi-byte characters correctly.
fn char_index_to_utf16_offset(s: &str, char_idx: usize) -> u32 {
    s.chars()
        .take(char_idx)
        .map(|c| c.len_utf16() as u32)
        .sum()
}
