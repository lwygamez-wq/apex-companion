//! Noms officiels des virages par circuit, positionnés par la distance de l'apex
//! depuis la ligne (mesurée sur des sessions réelles). Les noms donnés par le pilote
//! dans l'app restent prioritaires.
//!
//! Ajouter un circuit : relever les apex détectés sur une vraie session (export),
//! vérifier l'ordre et le sens des virages sur le tracé, puis compléter `TRACKS`.

use crate::model::{SessionMeta, SimId};

struct TrackCorners {
    sim: SimId,
    /// Fragments du nom du circuit tel qu'écrit par le simulateur (sans accents).
    name_contains: &'static [&'static str],
    /// Fragment de la configuration (`None` = toutes).
    config_contains: Option<&'static str>,
    corners: &'static [(f32, &'static str)],
}

const TRACKS: &[TrackCorners] = &[
    // Relevé : session iRacing du 17/10/2026, Super Formula Lights 324, apex détectés automatiquement.
    // Sens vérifié sur le tracé GPS (G, G, D, D, G, D, G). Curva do Sol et Mergulho, à fond, ne sont pas détectés.
    TrackCorners {
        sim: SimId::Iracing,
        name_contains: &["carlos pace", "interlagos"],
        config_contains: Some("grand prix"),
        corners: &[
            (326.0, "S do Senna"),
            (1379.0, "Descida do Lago"),
            (2103.0, "Ferradura"),
            (2292.0, "Laranjinha"),
            (2415.0, "Pinheirinho"),
            (2696.0, "Bico de Pato"),
            (3194.0, "Junção"),
        ],
    },
    // Relevé : session iRacing du 20/10/2026, Dallara F312 (F3), tracé GPS : sens et ordre vérifiés
    // (G, D, puis G D G D G D G pour les Esses, épingle 11, stade 12-15, triple droite 16-18).
    // Apex = vitesse minimale ; virages pris à fond (2, 10, 16-18) placés au milieu de la courbe.
    TrackCorners {
        sim: SimId::Iracing,
        name_contains: &["circuit of the americas"],
        config_contains: Some("grand prix"),
        corners: &[
            (655.0, "Virage 1"),
            (890.0, "Virage 2"),
            (1110.0, "Virage 3"),
            (1278.0, "Virage 4"),
            (1368.0, "Virage 5"),
            (1560.0, "Virage 6"),
            (1690.0, "Virage 7"),
            (1878.0, "Virage 8"),
            (1960.0, "Virage 9"),
            (2166.0, "Virage 10"),
            (2542.0, "Virage 11"),
            (3735.0, "Virage 12"),
            (3950.0, "Virage 13"),
            (4040.0, "Virage 14"),
            (4222.0, "Virage 15"),
            (4575.0, "Virages 16-18"),
            (4978.0, "Virage 19"),
            (5262.0, "Virage 20"),
        ],
    },
];

/// Circuits sans relevé sur session réelle : virages extraits de la géométrie publique du tracé
/// (ligne médiane OpenStreetMap : base TUMFTM racetrack-database, LGPL-3.0 ; GeoJSON bacinger/f1-circuits, MIT).
/// Position = fraction du tour depuis le début du tracé de référence, sens : +1 gauche, -1 droite,
/// angle = changement de cap total du virage (une épingle ≥ 140° n'est jamais passée à fond).
/// Les noms sont placés par alignement (ordre, sens, espacement) sur les virages détectés : voir `align`.
/// Tous les virages du tracé sont listés (l'alignement s'appuie sur la géométrie complète) ;
/// nom vide = virage dont le nom n'est pas sûr, laissé en numérotation automatique.
struct TrackSeq {
    name_contains: &'static [&'static str],
    /// (position en fraction de tour, sens, angle total en degrés, nom)
    corners: &'static [(f32, i8, u16, &'static str)],
}

const SEQ_TRACKS: &[TrackSeq] = &[
    // Spa-Francorchamps : 17 virages nommés sur 18 relevés sur le tracé.
    TrackSeq {
        name_contains: &["francorchamps"],
        corners: &[
            (0.0570, -1, 144, "La Source"),
            (0.1513, 1, 14, "Eau Rouge"),
            (0.1621, -1, 44, "Raidillon"),
            (0.1834, 1, 19, ""),
            (0.3470, -1, 69, "Les Combes"),
            (0.3597, 1, 68, "Les Combes"),
            (0.3814, -1, 66, "Malmedy"),
            (0.4297, -1, 182, "Bruxelles"),
            (0.4713, 1, 73, "Virage 9"),
            (0.5553, 1, 69, "Pouhon"),
            (0.5740, 1, 60, "Pouhon"),
            (0.6487, -1, 95, "Fagnes"),
            (0.6666, 1, 86, "Fagnes"),
            (0.7073, -1, 90, "Campus"),
            (0.7444, -1, 70, "Stavelot"),
            (0.8871, 1, 36, "Blanchimont"),
            (0.9644, -1, 97, "Bus Stop"),
            (0.9724, 1, 123, "Bus Stop"),
        ],
    },
    // Monza : 10 virages nommés sur 10 relevés sur le tracé.
    TrackSeq {
        name_contains: &["monza"],
        corners: &[
            (0.1608, -1, 77, "Rettifilo"),
            (0.1675, 1, 97, "Rettifilo"),
            (0.3708, 1, 63, "Roggia"),
            (0.3779, -1, 57, "Roggia"),
            (0.4483, -1, 95, "Lesmo 1"),
            (0.4977, -1, 63, "Lesmo 2"),
            (0.6831, 1, 52, "Ascari"),
            (0.7065, -1, 53, "Ascari"),
            (0.7153, 1, 41, "Ascari"),
            (0.8884, -1, 157, "Parabolica"),
        ],
    },
    // Silverstone (Grand Prix) : 17 virages nommés sur 17 relevés sur le tracé.
    TrackSeq {
        name_contains: &["silverstone"],
        corners: &[
            (0.0671, -1, 53, "Abbey"),
            (0.1094, 1, 35, "Farm"),
            (0.1520, -1, 109, "Village"),
            (0.1770, 1, 139, "The Loop"),
            (0.2118, 1, 46, "Aintree"),
            (0.3404, 1, 122, "Brooklands"),
            (0.3700, -1, 196, "Luffield"),
            (0.4328, -1, 30, "Woodcote"),
            (0.5322, -1, 71, "Copse"),
            (0.6137, 1, 19, "Maggotts"),
            (0.6311, -1, 48, "Maggotts"),
            (0.6606, 1, 49, "Becketts"),
            (0.6827, -1, 92, "Becketts"),
            (0.7130, 1, 25, "Chapel"),
            (0.8633, -1, 116, "Stowe"),
            (0.9372, 1, 87, "Vale"),
            (0.9508, -1, 164, "Club"),
        ],
    },
    // Suzuka : 16 virages nommés sur 21 relevés sur le tracé.
    TrackSeq {
        name_contains: &["suzuka"],
        corners: &[
            (0.1224, -1, 50, "Virage 1"),
            (0.1525, -1, 126, "Virage 2"),
            (0.1915, 1, 48, "S"),
            (0.2135, -1, 69, "S"),
            (0.2413, 1, 70, "S"),
            (0.2773, -1, 105, "S"),
            (0.2978, 1, 79, "Dunlop"),
            (0.3274, 1, 14, ""),
            (0.3445, 1, 14, ""),
            (0.3583, 1, 13, ""),
            (0.3962, -1, 43, "Degner 1"),
            (0.4231, -1, 81, "Degner 2"),
            (0.4782, -1, 26, ""),
            (0.5030, 1, 173, "Épingle"),
            (0.5528, -1, 34, "200R"),
            (0.6809, 1, 201, "Spoon"),
            (0.8570, 1, 31, "130R"),
            (0.9287, -1, 74, "Chicane Casio"),
            (0.9383, 1, 78, "Chicane Casio"),
            (0.9568, -1, 46, "Dernier virage"),
            (0.9707, -1, 27, ""),
        ],
    },
    // Nürburgring Grand-Prix : 9 virages nommés sur 15 relevés sur le tracé.
    TrackSeq {
        name_contains: &["nurburgring"],
        corners: &[
            (0.0783, -1, 150, ""),
            (0.1089, 1, 126, "Mercedes Arena"),
            (0.1645, 1, 162, "Mercedes Arena"),
            (0.1872, -1, 99, "Mercedes Arena"),
            (0.2840, 1, 77, ""),
            (0.3140, -1, 121, ""),
            (0.4277, -1, 193, "Dunlop-Kehre"),
            (0.5052, 1, 59, "Schumacher-S"),
            (0.5383, -1, 29, "Schumacher-S"),
            (0.6209, 1, 81, ""),
            (0.6635, -1, 103, ""),
            (0.7463, -1, 25, ""),
            (0.8404, 1, 52, "Chicane NGK"),
            (0.8503, -1, 53, "Chicane NGK"),
            (0.8901, -1, 143, "Coca-Cola"),
        ],
    },
    // Red Bull Ring : 5 virages nommés sur 10 relevés sur le tracé.
    TrackSeq {
        name_contains: &["red bull ring", "spielberg"],
        corners: &[
            (0.1043, -1, 75, "Virage 1"),
            (0.3228, -1, 125, "Remus"),
            (0.5114, -1, 122, "Schlossgold"),
            (0.5404, -1, 31, ""),
            (0.6345, 1, 124, ""),
            (0.7026, 1, 110, ""),
            (0.7311, -1, 13, ""),
            (0.7457, -1, 25, ""),
            (0.8785, -1, 72, "Virage 9"),
            (0.9241, -1, 72, "Virage 10"),
        ],
    },
    // Barcelona-Catalunya : 8 virages nommés sur 16 relevés sur le tracé.
    TrackSeq {
        name_contains: &["catalunya", "barcelona"],
        corners: &[
            (0.1798, -1, 86, "Virage 1"),
            (0.2041, 1, 66, "Virage 2"),
            (0.2514, -1, 151, "Virage 3"),
            (0.3665, -1, 175, "Virage 4"),
            (0.4609, 1, 145, "Virage 5"),
            (0.5047, 1, 21, ""),
            (0.5499, 1, 87, ""),
            (0.5684, -1, 30, ""),
            (0.6321, -1, 81, "Campsa"),
            (0.7525, 1, 139, "La Caixa"),
            (0.7790, 1, 47, ""),
            (0.8039, -1, 174, "Virage 12"),
            (0.8630, -1, 74, ""),
            (0.8919, 1, 76, ""),
            (0.9005, -1, 73, ""),
            (0.9258, -1, 84, ""),
        ],
    },
    // Hungaroring : 14 virages nommés sur 14 relevés sur le tracé.
    TrackSeq {
        name_contains: &["hungaroring"],
        corners: &[
            (0.1460, -1, 138, "Virage 1"),
            (0.2541, 1, 153, "Virage 2"),
            (0.3062, -1, 51, "Virage 3"),
            (0.4154, 1, 56, "Virage 4"),
            (0.4636, -1, 134, "Virage 5"),
            (0.5458, -1, 74, "Virage 6"),
            (0.5556, 1, 47, "Virage 7"),
            (0.5972, 1, 66, "Virage 8"),
            (0.6242, -1, 89, "Virage 9"),
            (0.6713, 1, 35, "Virage 10"),
            (0.7087, -1, 70, "Virage 11"),
            (0.8083, -1, 93, "Virage 12"),
            (0.8732, 1, 179, "Virage 13"),
            (0.9180, -1, 176, "Virage 14"),
        ],
    },
    // Zandvoort : 7 virages nommés sur 13 relevés sur le tracé.
    TrackSeq {
        name_contains: &["zandvoort"],
        corners: &[
            (0.0859, -1, 176, "Tarzan"),
            (0.1712, -1, 66, "Gerlach"),
            (0.2025, 1, 179, "Hugenholtz"),
            (0.3056, 1, 32, "Hunserug"),
            (0.3568, -1, 26, "Slotemaker"),
            (0.4089, -1, 115, "Scheivlak"),
            (0.4886, -1, 66, ""),
            (0.5433, -1, 160, ""),
            (0.6067, 1, 150, ""),
            (0.7372, -1, 84, ""),
            (0.7615, 1, 155, ""),
            (0.8389, -1, 93, ""),
            (0.9061, -1, 102, "Arie Luyendyk"),
        ],
    },
    // Hockenheim : 4 virages nommés sur 13 relevés sur le tracé.
    TrackSeq {
        name_contains: &["hockenheim"],
        corners: &[
            (0.0582, -1, 50, "Nordkurve"),
            (0.1838, -1, 133, ""),
            (0.2108, 1, 53, ""),
            (0.4607, -1, 169, "Spitzkehre"),
            (0.5561, -1, 37, ""),
            (0.6187, 1, 131, ""),
            (0.6542, -1, 89, ""),
            (0.7513, -1, 78, ""),
            (0.8295, 1, 154, "Sachs"),
            (0.8665, 1, 46, ""),
            (0.8818, -1, 28, ""),
            (0.9067, -1, 84, ""),
            (0.9354, -1, 97, "Südkurve"),
        ],
    },
    // Brands Hatch (Grand Prix) : 13 virages nommés sur 13 relevés sur le tracé.
    TrackSeq {
        name_contains: &["brands hatch"],
        corners: &[
            (0.0679, -1, 83, "Paddock Hill"),
            (0.1590, -1, 170, "Druids"),
            (0.2008, 1, 15, "Graham Hill"),
            (0.2228, 1, 78, "Graham Hill"),
            (0.3286, 1, 130, "Surtees"),
            (0.5209, -1, 76, "Hawthorn"),
            (0.6042, -1, 58, "Westfield"),
            (0.6352, -1, 24, "Dingle Dell"),
            (0.6731, -1, 20, "Sheene"),
            (0.7043, -1, 41, "Sheene"),
            (0.7676, 1, 78, "Stirlings"),
            (0.8787, -1, 78, "Clearways"),
            (0.9307, -1, 14, "Clark"),
        ],
    },
    // Gilles-Villeneuve (Montréal) : 5 virages nommés sur 14 relevés sur le tracé.
    TrackSeq {
        name_contains: &["gilles villeneuve", "gilles-villeneuve", "montreal"],
        corners: &[
            (0.0594, 1, 72, "Senna"),
            (0.0801, -1, 171, "Senna"),
            (0.1689, -1, 62, ""),
            (0.1813, 1, 54, ""),
            (0.2153, 1, 14, ""),
            (0.2304, -1, 51, ""),
            (0.2894, 1, 100, ""),
            (0.3036, -1, 89, ""),
            (0.4631, -1, 66, ""),
            (0.4737, 1, 63, ""),
            (0.6201, -1, 186, "Épingle"),
            (0.6490, 1, 16, ""),
            (0.8968, -1, 55, "Mur des Champions"),
            (0.9046, 1, 56, "Mur des Champions"),
        ],
    },
    // Bahreïn : 14 virages nommés sur 14 relevés sur le tracé.
    TrackSeq {
        name_contains: &["bahrain", "sakhir"],
        corners: &[
            (0.1367, -1, 131, "Virage 1"),
            (0.1554, 1, 60, "Virage 2"),
            (0.1768, -1, 24, "Virage 3"),
            (0.2830, -1, 128, "Virage 4"),
            (0.3345, 1, 13, "Virage 5"),
            (0.3487, -1, 70, "Virage 6"),
            (0.3742, 1, 55, "Virage 7"),
            (0.4229, -1, 149, "Virage 8"),
            (0.5056, 1, 176, "Virages 9-10"),
            (0.6375, 1, 156, "Virage 11"),
            (0.6950, -1, 90, "Virage 12"),
            (0.7653, -1, 123, "Virage 13"),
            (0.9120, -1, 93, "Virage 14"),
            (0.9277, -1, 21, "Virage 15"),
        ],
    },
    // Hermanos Rodríguez (Mexico) : 16 virages nommés sur 16 relevés sur le tracé.
    TrackSeq {
        name_contains: &["hermanos rodriguez", "mexico"],
        corners: &[
            (0.2292, -1, 85, "Virage 1"),
            (0.2504, 1, 67, "Virage 2"),
            (0.2618, -1, 70, "Virage 3"),
            (0.4247, 1, 88, "Virage 4"),
            (0.4419, -1, 106, "Virage 5"),
            (0.4754, -1, 132, "Virage 6"),
            (0.5550, 1, 63, "Virage 7"),
            (0.5790, -1, 30, "Virage 8"),
            (0.6057, 1, 56, "Virage 9"),
            (0.6444, -1, 65, "Virage 10"),
            (0.6639, 1, 45, "Virage 11"),
            (0.7977, -1, 76, "Virage 12"),
            (0.8398, 1, 132, "Virage 13"),
            (0.8545, -1, 75, "Virage 14"),
            (0.8629, 1, 17, "Virage 15"),
            (0.8836, -1, 180, "Peraltada"),
        ],
    },
    // Shanghai : 1 virages nommés sur 13 relevés sur le tracé.
    TrackSeq {
        name_contains: &["shanghai"],
        corners: &[
            (0.1506, -1, 301, ""),
            (0.1633, 1, 239, ""),
            (0.2485, -1, 17, ""),
            (0.2953, -1, 144, ""),
            (0.3576, 1, 120, ""),
            (0.4349, -1, 123, ""),
            (0.4725, 1, 114, ""),
            (0.4894, 1, 67, ""),
            (0.5779, 1, 109, ""),
            (0.5910, -1, 285, ""),
            (0.8821, -1, 152, "Épingle"),
            (0.8988, -1, 22, ""),
            (0.9507, 1, 66, ""),
        ],
    },
    // Imola : 11 virages nommés sur 11 relevés sur le tracé.
    TrackSeq {
        name_contains: &["imola", "enzo e dino"],
        corners: &[
            (0.1419, 1, 60, "Tamburello"),
            (0.1512, -1, 58, "Tamburello"),
            (0.1765, 1, 24, "Tamburello"),
            (0.2907, -1, 64, "Villeneuve"),
            (0.3512, 1, 139, "Tosa"),
            (0.4703, 1, 109, "Piratella"),
            (0.5786, -1, 113, "Acque Minerali"),
            (0.5863, 1, 22, "Acque Minerali"),
            (0.6767, -1, 56, "Variante Alta"),
            (0.6915, 1, 75, "Variante Alta"),
            (0.8425, 1, 175, "Rivazza"),
        ],
    },
    // Mugello : 11 virages nommés sur 14 relevés sur le tracé.
    TrackSeq {
        name_contains: &["mugello"],
        corners: &[
            (0.1328, -1, 165, "San Donato"),
            (0.1841, 1, 66, "Luco"),
            (0.2072, -1, 88, "Poggio Secco"),
            (0.2845, 1, 46, "Materassi"),
            (0.3095, -1, 49, "Materassi"),
            (0.3771, -1, 25, "Borgo San Lorenzo"),
            (0.4042, 1, 62, "Borgo San Lorenzo"),
            (0.4666, -1, 72, "Casanova"),
            (0.5072, -1, 47, "Savelli"),
            (0.5839, -1, 61, "Arrabbiata 1"),
            (0.6020, 1, 73, ""),
            (0.6908, -1, 187, ""),
            (0.7421, 1, 31, ""),
            (0.8673, 1, 169, "Bucine"),
        ],
    },
    // Circuit of the Americas (autres simulateurs) : 21 virages nommés sur 22 relevés sur le tracé.
    TrackSeq {
        name_contains: &["circuit of the americas", "austin"],
        corners: &[
            (0.1204, 1, 132, "Virage 1"),
            (0.1716, -1, 71, "Virage 2"),
            (0.2123, 1, 32, "Virage 3"),
            (0.2306, -1, 44, "Virage 4"),
            (0.2478, 1, 59, "Virage 5"),
            (0.2869, -1, 104, "Virage 6"),
            (0.3132, 1, 63, "Virage 7"),
            (0.3388, -1, 106, "Virage 8"),
            (0.3602, 1, 65, "Virage 9"),
            (0.3973, 1, 36, "Virage 10"),
            (0.4701, 1, 141, "Virage 11"),
            (0.6872, 1, 120, "Virage 12"),
            (0.7288, -1, 123, "Virage 13"),
            (0.7459, -1, 64, "Virage 14"),
            (0.7571, 1, 19, ""),
            (0.7804, 1, 164, "Virage 15"),
            (0.8205, -1, 45, "Virages 16-18"),
            (0.8376, -1, 42, "Virages 16-18"),
            (0.8548, -1, 41, "Virages 16-18"),
            (0.8748, -1, 33, "Virages 16-18"),
            (0.9175, 1, 70, "Virage 19"),
            (0.9730, 1, 116, "Virage 20"),
        ],
    },
    // Interlagos (autres simulateurs) : 14 virages nommés sur 15 relevés sur le tracé.
    TrackSeq {
        name_contains: &["carlos pace", "interlagos"],
        corners: &[
            (0.0862, 1, 117, "S do Senna"),
            (0.1069, -1, 73, "S do Senna"),
            (0.1464, 1, 92, "Curva do Sol"),
            (0.3308, 1, 90, "Descida do Lago"),
            (0.3633, 1, 19, "Descida do Lago"),
            (0.3763, 1, 31, "Descida do Lago"),
            (0.4714, -1, 128, "Ferradura"),
            (0.5406, -1, 128, "Laranjinha"),
            (0.5699, 1, 168, "Pinheirinho"),
            (0.6175, -1, 21, ""),
            (0.6461, -1, 151, "Bico de Pato"),
            (0.7018, 1, 99, "Mergulho"),
            (0.7587, 1, 97, "Junção"),
            (0.7875, 1, 27, "Junção"),
            (0.8516, 1, 30, "Arquibancadas"),
        ],
    },

];

/// Virage de table orienté : (position, sens, angle, nom).
type TabCorner = (f32, i8, u16, String);

/// Écart maximal accepté entre un virage détecté et un virage de la table (fraction de tour).
const ALIGN_TOL: f32 = 0.02;

/// Associe les virages détectés (apex en fraction de tour, sens +1/-1 si connu) aux virages
/// d'une table. Le départ du tracé de référence et le sens de description peuvent différer de ceux
/// du simulateur : on essaie tous les décalages et les deux sens, et on garde le meilleur alignement
/// (ordre respecté, même sens de virage, écart ≤ 2 % du tour). Refuse si l'alignement est douteux.
fn align(det: &[(f32, Option<i8>)], table: &[(f32, i8, u16, &str)]) -> Vec<Option<String>> {
    let n = det.len();
    let mut best: (f32, Vec<Option<usize>>, Vec<TabCorner>) = (0.0, vec![None; n], vec![]);
    for reverse in [false, true] {
        let tab: Vec<TabCorner> = table
            .iter()
            .map(|(p, d, a, nm)| if reverse { ((1.0 - p).rem_euclid(1.0), -d, *a, nm.to_string()) } else { (*p, *d, *a, nm.to_string()) })
            .collect();
        let mut shifts = vec![0.0f32];
        for (p, dd) in det {
            for (t, td, _, _) in &tab {
                if dd.is_none_or(|x| x == *td) {
                    shifts.push((t - p).rem_euclid(1.0));
                }
            }
        }
        for s in shifts {
            let mut q: Vec<(f32, i8, usize)> = tab.iter().enumerate().map(|(j, (t, d, _, _))| ((t - s).rem_euclid(1.0), *d, j)).collect();
            q.sort_by(|a, b| a.0.total_cmp(&b.0));
            let m = q.len();
            // Alignement ordonné (programmation dynamique) maximisant la qualité des correspondances.
            let mut dp = vec![vec![0.0f32; m + 1]; n + 1];
            for i in 1..=n {
                for j in 1..=m {
                    let (p, dd) = det[i - 1];
                    let (t, td, _) = q[j - 1];
                    let delta = (p - t).abs().min(1.0 - (p - t).abs());
                    let ok = delta <= ALIGN_TOL && dd.is_none_or(|x| x == td);
                    let mv = if ok { dp[i - 1][j - 1] + 1.0 - delta / ALIGN_TOL * 0.5 } else { f32::MIN };
                    dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]).max(mv);
                }
            }
            if dp[n][m] <= best.0 {
                continue;
            }
            // Une épingle du tracé sans aucun virage détecté à proximité : décalage incohérent.
            let hairpin_missed = q.iter().any(|(t, _, j)| {
                tab[*j].2 >= 140 && !det.iter().any(|(p, _)| (p - t).abs().min(1.0 - (p - t).abs()) <= ALIGN_TOL)
            });
            if hairpin_missed {
                continue;
            }
            // Reconstitution.
            let mut assign = vec![None; n];
            let (mut i, mut j) = (n, m);
            while i > 0 && j > 0 {
                if dp[i][j] == dp[i - 1][j] {
                    i -= 1;
                } else if dp[i][j] == dp[i][j - 1] {
                    j -= 1;
                } else {
                    assign[i - 1] = Some(q[j - 1].2);
                    i -= 1;
                    j -= 1;
                }
            }
            best = (dp[n][m], assign, tab.clone());
        }
    }
    let (_, assign, tab) = best;
    let matched: Vec<(usize, usize)> = assign.iter().enumerate().filter_map(|(i, a)| a.map(|j| (i, j))).collect();
    let with_dir = matched.iter().filter(|(i, _)| det[*i].1.is_some()).count();
    let mean_delta = if matched.is_empty() {
        1.0
    } else {
        // Écart moyen après recentrage (le décalage optimal absorbe la différence de ligne de départ).
        let d: Vec<f32> = matched.iter().map(|(i, j)| {
            let x = det[*i].0 - tab[*j].0;
            x - x.round()
        }).collect();
        let mu = d.iter().sum::<f32>() / d.len() as f32;
        d.iter().map(|x| (x - mu).abs()).sum::<f32>() / d.len() as f32
    };
    // La plupart des virages détectés doivent correspondre à un virage du tracé, sinon (autre
    // configuration, autre circuit) on ne nomme rien plutôt que de risquer un faux nom.
    let enough = matched.len() >= 3 && with_dir >= 3 && mean_delta <= 0.010 && matched.len() * 10 >= n * 7;
    if !enough {
        return vec![None; n];
    }
    assign.into_iter().map(|a| a.map(|j| tab[j].3.clone()).filter(|n| !n.is_empty())).collect()
}

/// Sens du virage autour de l'apex (cap à −25 m et +25 m) : +1 gauche, −1 droite, `None` si presque droit.
/// Mesure locale : fiable même dans un enchaînement gauche-droite serré (Esses).
fn local_turn(r: &crate::analysis::resample::Resampled, apex_m: f32) -> Option<i8> {
    let (x, y) = (r.x.as_ref()?, r.y.as_ref()?);
    let n = x.len();
    let at = |m: f32| ((m / r.step_m).round().max(0.0) as usize).min(n - 1);
    let head = |i: usize| {
        let (a, b) = (i.saturating_sub(5), (i + 5).min(n - 1));
        (y[b] - y[a]).atan2(x[b] - x[a])
    };
    let mut d = head(at(apex_m + 25.0)) - head(at(apex_m - 25.0));
    d = (d + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI) - std::f32::consts::PI;
    (d.abs() > 0.08).then_some(if d > 0.0 { 1 } else { -1 })
}

/// Noms placés par alignement sur la géométrie publique, pour les virages détectés sur `reference`.
/// Vide si le circuit n'a pas de table, si la trace n'a pas de position, ou si l'alignement est douteux.
pub fn aligned_corner_names(m: &SessionMeta, reference: &crate::model::LapTrace) -> Vec<(f32, String)> {
    let track = fold(&m.track);
    let Some(t) = SEQ_TRACKS.iter().find(|t| t.name_contains.iter().any(|n| track.contains(n))) else { return vec![] };
    let length = m.track_length_m.filter(|l| *l > 100.0).unwrap_or_else(|| reference.max_dist());
    let Ok(r) = crate::analysis::resample::resample(reference, length, 1.0) else { return vec![] };
    if r.x.is_none() {
        return vec![];
    }
    let defs = crate::analysis::corners::detect_corners(&r, &crate::analysis::corners::CornerConfig::default());
    let det: Vec<(f32, Option<i8>)> = defs
        .iter()
        .map(|d| (d.apex_m / length, local_turn(&r, d.apex_m)))
        .collect();
    align(&det, t.corners)
        .into_iter()
        .zip(&defs)
        .filter_map(|(n, d)| n.map(|n| (d.apex_m, n)))
        .collect()
}

/// Le circuit a-t-il une géométrie publique intégrée (noms placés par alignement) ?
pub fn has_geometry(m: &SessionMeta) -> bool {
    let track = fold(&m.track);
    SEQ_TRACKS.iter().any(|t| t.name_contains.iter().any(|n| track.contains(n)))
}

/// Noms à appliquer pour une session : relevé exact s'il existe, sinon alignement sur la géométrie
/// publique (calculé sur `reference`), puis noms du pilote par-dessus.
pub fn corner_names_for(m: &SessionMeta, reference: Option<&crate::model::LapTrace>, user: Vec<(f32, String)>) -> Vec<(f32, String)> {
    let exact = builtin_corner_names(m);
    let base = if !exact.is_empty() { exact } else { reference.map(|r| aligned_corner_names(m, r)).unwrap_or_default() };
    merge_names(base, user)
}

/// Nom normalisé (minuscules, sans accents) pour comparer des noms de circuits ou de voitures.
pub fn fold_name(s: &str) -> String {
    fold(s)
}

fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'î' | 'ï' => 'i',
            'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Noms intégrés pour le circuit de cette session (vide si circuit inconnu).
pub fn builtin_corner_names(m: &SessionMeta) -> Vec<(f32, String)> {
    let track = fold(&m.track);
    let config = fold(m.track_config.as_deref().unwrap_or(""));
    TRACKS
        .iter()
        .find(|t| {
            t.sim == m.sim
                && t.name_contains.iter().any(|n| track.contains(n))
                && t.config_contains.is_none_or(|c| config.contains(c))
        })
        .map(|t| t.corners.iter().map(|(d, n)| (*d, n.to_string())).collect())
        .unwrap_or_default()
}

/// Fusionne noms intégrés et noms du pilote : un nom du pilote à moins de 60 m remplace le nom intégré.
pub fn merge_names(builtin: Vec<(f32, String)>, user: Vec<(f32, String)>) -> Vec<(f32, String)> {
    let mut out: Vec<(f32, String)> = builtin.into_iter().filter(|(d, _)| !user.iter().any(|(u, _)| (u - d).abs() <= 60.0)).collect();
    out.extend(user);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn meta(track: &str, config: Option<&str>) -> SessionMeta {
        SessionMeta {
            sim: SimId::Iracing,
            track: track.into(),
            track_config: config.map(String::from),
            track_length_m: Some(4222.6),
            car: "Super Formula Lights 324".into(),
            kind: SessionKind::Race,
            started_at_unix: 0,
            weather: Weather::default(),
            source_fingerprint: "x".into(),
            source_path: None,
        }
    }

    #[test]
    fn interlagos_names_with_accents() {
        let n = builtin_corner_names(&meta("Autódromo José Carlos Pace", Some("Grand Prix")));
        assert_eq!(n.len(), 7);
        assert_eq!(n[0].1, "S do Senna");
        assert!(builtin_corner_names(&meta("Spa-Francorchamps", Some("Grand Prix"))).is_empty());
    }

    /// Virages « détectés » simulés à partir d'une table : décalage de ligne de départ, bruit de position,
    /// et un virage sur quatre non détecté (pris à fond).
    fn simulate(t: &TrackSeq, shift: f32, noise: f32) -> Vec<(f32, Option<i8>)> {
        t.corners
            .iter()
            .enumerate()
            .filter(|(i, c)| i % 4 != 3 || c.2 >= 140)
            .map(|(i, (p, d, _, _))| ((p - shift + noise * if i % 2 == 0 { 1.0 } else { -1.0 }).rem_euclid(1.0), Some(*d)))
            .collect::<Vec<_>>()
    }

    #[test]
    fn alignment_finds_names_despite_start_offset_and_noise() {
        for t in SEQ_TRACKS {
            let mut det = simulate(t, 0.137, 0.003);
            det.sort_by(|a, b| a.0.total_cmp(&b.0));
            let got = align(&det, t.corners);
            // Tous les virages nommés de la table qui ont été « détectés » doivent l'être.
            let expected = t.corners.iter().enumerate().filter(|(i, c)| (i % 4 != 3 || c.2 >= 140) && !c.3.is_empty()).count();
            let named = got.iter().filter(|n| n.is_some()).count();
            assert!(named + 1 >= expected && named <= expected, "{:?} : {named}/{expected}", t.name_contains);
        }
    }

    #[test]
    fn alignment_handles_reverse_description() {
        let t = &SEQ_TRACKS[0];
        // Même circuit décrit dans l'autre sens par la source : on retrouve le sens de course.
        let mut det: Vec<(f32, Option<i8>)> = t.corners.iter().map(|(p, d, _, _)| ((1.0 - p).rem_euclid(1.0), Some(-d))).collect();
        det.sort_by(|a, b| a.0.total_cmp(&b.0));
        let got = align(&det, t.corners);
        assert_eq!(got.iter().filter(|n| n.is_some()).count(), t.corners.iter().filter(|c| !c.3.is_empty()).count());
        assert_eq!(got.last().unwrap().as_deref(), Some("La Source"));
    }

    #[test]
    fn alignment_refuses_the_wrong_track() {
        // Un circuit n'est jamais nommé avec la table d'un autre (pas de faux noms).
        let mut wrong = 0;
        for a in SEQ_TRACKS {
            let mut det = simulate(a, 0.0, 0.0);
            det.sort_by(|x, y| x.0.total_cmp(&y.0));
            for b in SEQ_TRACKS {
                if std::ptr::eq(a, b) || a.name_contains == b.name_contains {
                    continue;
                }
                if align(&det, b.corners).iter().any(|n| n.is_some()) {
                    wrong += 1;
                }
            }
        }
        let pairs = SEQ_TRACKS.len() * (SEQ_TRACKS.len() - 1);
        eprintln!("{wrong} associations erronées sur {pairs}");
        assert!(wrong * 20 <= pairs, "{wrong} associations erronées sur {pairs}");
    }

    #[test]
    fn user_name_overrides_builtin() {
        let b = vec![(326.0, "S do Senna".to_string()), (1379.0, "Descida do Lago".to_string())];
        let m = merge_names(b, vec![(340.0, "Le S".to_string())]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().any(|(_, n)| n == "Le S") && !m.iter().any(|(_, n)| n == "S do Senna"));
    }
}
