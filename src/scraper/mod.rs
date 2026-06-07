pub mod arxiv;
pub mod crossref;
pub mod pubmed;
pub mod semantic_scholar;

use async_trait::async_trait;

use crate::filters::FilterSet;
use crate::models::SearchResult;

#[async_trait]
pub trait PaperSource: Send + Sync {
    async fn fetch(&self, filters: &FilterSet, page: u32) -> anyhow::Result<SearchResult>;
    fn name(&self) -> &'static str;
}

pub use arxiv::ArxivSource;
pub use crossref::CrossRefSource;
pub use pubmed::PubMedSource;
pub use semantic_scholar::SemanticScholarSource;

// ─── Chemical formula expansion ──────────────────────────────────────────────
// Queries like "LiNe+" case-fold to "line" in every text-search API, returning
// unrelated results. Detect pure-symbol formulas (no stoichiometric digits) and
// replace element symbols with their full names, e.g. "LiNe+" → "lithium neon".
// Formulas with embedded digits (CO2, H2O, Fe3O4) are left unchanged — they don't
// collide with common English words and the digit already disambiguates them.

pub(super) fn expand_chemical_formula(s: &str) -> Option<String> {
    // Strip trailing charge suffix: +, -, 2+, 2-, etc.
    let without_charge = strip_charge(s);

    // If embedded digits remain, this is a stoichiometric formula — don't expand.
    if without_charge.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }

    // Parse element symbols; every token must be a known element.
    let elems = parse_elements(without_charge)?;
    if elems.len() < 2 {
        return None;
    }
    let names: Vec<&str> = elems.iter()
        .map(|sym| elem_name(sym))
        .collect::<Option<_>>()?;

    // Deduplicate while preserving order (e.g. "LiLi" → "lithium").
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<&str> = names.into_iter().filter(|n| seen.insert(*n)).collect();
    Some(unique.join(" "))
}

fn strip_charge(s: &str) -> &str {
    let b = s.as_bytes();
    let n = b.len();
    if n == 0 { return s; }
    // Optional digit followed by + or -, at the very end
    if b[n-1] == b'+' || b[n-1] == b'-' {
        let cut = if n >= 2 && b[n-2].is_ascii_digit() { n - 2 } else { n - 1 };
        return &s[..cut];
    }
    s
}

fn parse_elements(s: &str) -> Option<Vec<String>> {
    let mut result = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_uppercase() { return None; }
        let mut sym = String::new();
        sym.push(chars[i]);
        i += 1;
        // At most one lowercase letter follows (element symbols are 1-2 chars)
        if i < chars.len() && chars[i].is_ascii_lowercase() {
            sym.push(chars[i]);
            i += 1;
        }
        result.push(sym);
    }
    if result.is_empty() { None } else { Some(result) }
}

fn elem_name(sym: &str) -> Option<&'static str> {
    Some(match sym {
        "H"  => "hydrogen",      "He" => "helium",        "Li" => "lithium",
        "Be" => "beryllium",     "B"  => "boron",         "C"  => "carbon",
        "N"  => "nitrogen",      "O"  => "oxygen",        "F"  => "fluorine",
        "Ne" => "neon",          "Na" => "sodium",        "Mg" => "magnesium",
        "Al" => "aluminum",      "Si" => "silicon",       "P"  => "phosphorus",
        "S"  => "sulfur",        "Cl" => "chlorine",      "Ar" => "argon",
        "K"  => "potassium",     "Ca" => "calcium",       "Sc" => "scandium",
        "Ti" => "titanium",      "V"  => "vanadium",      "Cr" => "chromium",
        "Mn" => "manganese",     "Fe" => "iron",          "Co" => "cobalt",
        "Ni" => "nickel",        "Cu" => "copper",        "Zn" => "zinc",
        "Ga" => "gallium",       "Ge" => "germanium",     "As" => "arsenic",
        "Se" => "selenium",      "Br" => "bromine",       "Kr" => "krypton",
        "Rb" => "rubidium",      "Sr" => "strontium",     "Y"  => "yttrium",
        "Zr" => "zirconium",     "Nb" => "niobium",       "Mo" => "molybdenum",
        "Tc" => "technetium",    "Ru" => "ruthenium",     "Rh" => "rhodium",
        "Pd" => "palladium",     "Ag" => "silver",        "Cd" => "cadmium",
        "In" => "indium",        "Sn" => "tin",           "Sb" => "antimony",
        "Te" => "tellurium",     "I"  => "iodine",        "Xe" => "xenon",
        "Cs" => "cesium",        "Ba" => "barium",        "La" => "lanthanum",
        "Ce" => "cerium",        "Pr" => "praseodymium",  "Nd" => "neodymium",
        "Pm" => "promethium",    "Sm" => "samarium",      "Eu" => "europium",
        "Gd" => "gadolinium",    "Tb" => "terbium",       "Dy" => "dysprosium",
        "Ho" => "holmium",       "Er" => "erbium",        "Tm" => "thulium",
        "Yb" => "ytterbium",     "Lu" => "lutetium",      "Hf" => "hafnium",
        "Ta" => "tantalum",      "W"  => "tungsten",      "Re" => "rhenium",
        "Os" => "osmium",        "Ir" => "iridium",       "Pt" => "platinum",
        "Au" => "gold",          "Hg" => "mercury",       "Tl" => "thallium",
        "Pb" => "lead",          "Bi" => "bismuth",       "Po" => "polonium",
        "At" => "astatine",      "Rn" => "radon",         "Fr" => "francium",
        "Ra" => "radium",        "Ac" => "actinium",      "Th" => "thorium",
        "Pa" => "protactinium",  "U"  => "uranium",       "Np" => "neptunium",
        "Pu" => "plutonium",     "Am" => "americium",     "Cm" => "curium",
        "Bk" => "berkelium",     "Cf" => "californium",   "Es" => "einsteinium",
        "Fm" => "fermium",       "Md" => "mendelevium",   "No" => "nobelium",
        "Lr" => "lawrencium",    "Rf" => "rutherfordium", "Db" => "dubnium",
        "Sg" => "seaborgium",    "Bh" => "bohrium",       "Hs" => "hassium",
        "Mt" => "meitnerium",    "Ds" => "darmstadtium",  "Rg" => "roentgenium",
        "Cn" => "copernicium",   "Nh" => "nihonium",      "Fl" => "flerovium",
        "Mc" => "moscovium",     "Lv" => "livermorium",   "Ts" => "tennessine",
        "Og" => "oganesson",
        _ => return None,
    })
}

// ─── Shared LaTeX cleaner ────────────────────────────────────────────────────
// Used by arxiv (raw LaTeX in Atom feed) and crossref (jats:tex-math content).

pub(super) fn clean_latex(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '$' => {}
            '\\' => latex_cmd(&mut chars, &mut result),
            '_' => {
                let group = latex_group(&mut chars);
                for gc in group.chars() {
                    result.push(to_subscript(gc).unwrap_or(gc));
                }
            }
            '^' => {
                let group = latex_group(&mut chars);
                for gc in group.chars() {
                    result.push(to_superscript(gc).unwrap_or(gc));
                }
            }
            '{' | '}' => {}
            c => result.push(c),
        }
    }

    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn to_subscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄',
        '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
        '+' => '₊', '-' => '₋',
        'a' => 'ₐ', 'e' => 'ₑ', 'o' => 'ₒ', 'x' => 'ₓ',
        'h' => 'ₕ', 'k' => 'ₖ', 'l' => 'ₗ', 'm' => 'ₘ',
        'n' => 'ₙ', 'p' => 'ₚ', 's' => 'ₛ', 't' => 'ₜ',
        _ => return None,
    })
}

pub(super) fn to_superscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴',
        '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',
        '+' => '⁺', '-' => '⁻',
        'n' => 'ⁿ', 'i' => 'ⁱ',
        _ => return None,
    })
}

fn latex_group<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> String {
    match chars.peek() {
        Some(&'{') => {
            chars.next();
            let mut group = String::new();
            let mut depth = 1i32;
            for c in chars.by_ref() {
                match c {
                    '{' => { depth += 1; group.push(c); }
                    '}' => {
                        depth -= 1;
                        if depth == 0 { break; }
                        group.push(c);
                    }
                    _ => group.push(c),
                }
            }
            group
        }
        Some(_) => chars.next().unwrap().to_string(),
        None => String::new(),
    }
}

fn latex_cmd<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>, out: &mut String) {
    let cmd: String = if chars.peek().map_or(false, |c| !c.is_alphabetic()) {
        let c = chars.next().unwrap();
        c.to_string()
    } else {
        let mut name = String::new();
        while chars.peek().map_or(false, |c| c.is_alphabetic()) {
            name.push(chars.next().unwrap());
        }
        if chars.peek() == Some(&' ') {
            chars.next();
        }
        name
    };

    match cmd.as_str() {
        "\\" | " " | "," | ";" | ":" | "!" | "par" | "newline" => out.push(' '),
        "(" | ")" | "[" | "]" => {}
        "text" | "mathrm" | "textrm" | "textbf" | "textit" | "emph" | "mathbf"
        | "mathit" | "mathsf" | "mathtt" | "mbox" | "hbox" | "rm" | "it" | "bf"
        | "sf" | "tt" | "sc" | "sl" | "underline" | "overline" => {}
        "alpha"                        => out.push('α'),
        "beta"                         => out.push('β'),
        "gamma"                        => out.push('γ'),
        "delta"                        => out.push('δ'),
        "epsilon" | "varepsilon"       => out.push('ε'),
        "zeta"                         => out.push('ζ'),
        "eta"                          => out.push('η'),
        "theta" | "vartheta"           => out.push('θ'),
        "iota"                         => out.push('ι'),
        "kappa"                        => out.push('κ'),
        "lambda"                       => out.push('λ'),
        "mu"                           => out.push('μ'),
        "nu"                           => out.push('ν'),
        "xi"                           => out.push('ξ'),
        "pi" | "varpi"                 => out.push('π'),
        "rho" | "varrho"               => out.push('ρ'),
        "sigma" | "varsigma"           => out.push('σ'),
        "tau"                          => out.push('τ'),
        "upsilon"                      => out.push('υ'),
        "phi" | "varphi"               => out.push('φ'),
        "chi"                          => out.push('χ'),
        "psi"                          => out.push('ψ'),
        "omega"                        => out.push('ω'),
        "Gamma"                        => out.push('Γ'),
        "Delta"                        => out.push('Δ'),
        "Theta"                        => out.push('Θ'),
        "Lambda"                       => out.push('Λ'),
        "Xi"                           => out.push('Ξ'),
        "Pi"                           => out.push('Π'),
        "Sigma"                        => out.push('Σ'),
        "Upsilon"                      => out.push('Υ'),
        "Phi"                          => out.push('Φ'),
        "Psi"                          => out.push('Ψ'),
        "Omega"                        => out.push('Ω'),
        "cdot" | "bullet"              => out.push('·'),
        "cdots" | "ldots" | "dots"     => out.push('…'),
        "times"                        => out.push('×'),
        "div"                          => out.push('÷'),
        "pm"                           => out.push('±'),
        "mp"                           => out.push('∓'),
        "infty"                        => out.push('∞'),
        "rightarrow" | "to"            => out.push('→'),
        "leftarrow" | "gets"           => out.push('←'),
        "Rightarrow"                   => out.push('⇒'),
        "Leftarrow"                    => out.push('⇐'),
        "leftrightarrow"               => out.push('↔'),
        "sim"                          => out.push('~'),
        "approx"                       => out.push('≈'),
        "geq" | "ge"                   => out.push('≥'),
        "leq" | "le"                   => out.push('≤'),
        "neq" | "ne"                   => out.push('≠'),
        "in"                           => out.push('∈'),
        "subset"                       => out.push('⊂'),
        "supset"                       => out.push('⊃'),
        "cup"                          => out.push('∪'),
        "cap"                          => out.push('∩'),
        "partial"                      => out.push('∂'),
        "nabla"                        => out.push('∇'),
        "sum"                          => out.push('∑'),
        "prod"                         => out.push('∏'),
        "int"                          => out.push('∫'),
        "sqrt"                         => out.push('√'),
        "forall"                       => out.push('∀'),
        "exists"                       => out.push('∃'),
        "&"                            => out.push('&'),
        "%"                            => out.push('%'),
        "#"                            => out.push('#'),
        "_"                            => out.push('_'),
        "^"                            => out.push('^'),
        _ => {}
    }
}
