//! Made-up names for famous people a shared club has more of than this player's own tables (a club started with other
//! game files: mp/club.rs take_shared_tables). Every name is our own, put together from the lists below.

const FIRST: [&str; 32] = [
    "Ada", "Bea", "Cal", "Dot", "Eli", "Fay", "Gil", "Hal", "Ira", "Jo", "Kit", "Lou", "Mae", "Ned", "Ola", "Pip", "Quin",
    "Ray", "Sid", "Tia", "Uma", "Viv", "Wes", "Xan", "Yul", "Zoe", "Bo", "Cy", "Di", "Gus", "Liv", "Moe",
];

const LAST: [&str; 29] = [
    "Birdwhistle", "Chipworth", "Divotson", "Eaglebury", "Fairbanks", "Greenleaf", "Hookham", "Ironmonger", "Linksley",
    "Mulligan", "Niblick", "Overdrive", "Puttering", "Roughwood", "Sandtrap", "Teewell", "Underpar", "Wedgewood",
    "Yardley", "Backswing", "Clubhouse", "Doglegg", "Flagstick", "Gimme", "Holloway", "Lofton", "Pinhigh", "Spinwell",
    "Waggle",
];

const STAGE: [&str; 11] = [
    "Blaze", "Starla", "Councillor", "Giggles", "Glamora", "Coach", "Chuckles", "Famously", "Princess", "Riff", "Hoops",
];

/// A pro's name for row `i` (different for every row up to 928).
pub fn pro(i: usize) -> String {
    format!("{} {}", FIRST[i % FIRST.len()], LAST[(i / FIRST.len() + i * 7) % LAST.len()])
}

/// A celebrity's name for row `i`.
pub fn celebrity(i: usize) -> String {
    format!("{} {}", STAGE[i % STAGE.len()], LAST[(i * 5 + 3) % LAST.len()])
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_names_differ() {
        let names: std::collections::HashSet<String> = (0..200).map(super::pro).collect();
        assert_eq!(names.len(), 200);
    }
}
