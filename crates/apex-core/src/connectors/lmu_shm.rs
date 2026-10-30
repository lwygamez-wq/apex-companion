//! Disposition de la mémoire partagée native de Le Mans Ultimate (`LMU_Data`).
//!
//! Le jeu la publie lui-même (aucun plugin à installer) quand *Réglages → Gameplay → Enable Plugins*
//! est activé. Disposition tirée de l'en-tête officiel `SharedMemoryInterface.hpp` livré par Studio 397
//! dans le dossier `Support\SharedMemoryInterface` du jeu, recoupée avec deux implémentations
//! open source sous licence MIT : pyLMUSharedMemory (TinyPedal) et lmu-pitwall (vérifié sur LMU 1.4).
//!
//! Les tailles sont vérifiées à la compilation : si Studio 397 ajoute un champ, la compilation casse
//! ici au lieu de lire silencieusement des valeurs décalées.
//!
//! Pas de compteur de version dans `LMU_Data` : la cohérence d'une copie est contrôlée en relisant,
//! après la copie, deux horloges que le jeu écrit à chaque mise à jour (témoins).

#![allow(non_snake_case, dead_code)]

/// Nom de la projection Windows publiée par le jeu.
pub const LMU_DATA_NAME: &str = "LMU_Data";
/// Emplacements de voitures dans `LMU_Data` (104, pas les 128 de l'ancien plugin rFactor 2).
pub const MAX_MAPPED_VEHICLES: usize = 104;

macro_rules! assert_size {
    ($t:ty, $n:expr) => {
        const _: () = assert!(std::mem::size_of::<$t>() == $n);
    };
}

#[repr(C, packed(4))]
#[derive(Clone, Copy, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
assert_size!(Vec3, 24);

/// `TelemWheelV01`.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct Wheel {
    pub mSuspensionDeflection: f64,
    pub mRideHeight: f64,
    pub mSuspForce: f64,
    pub mBrakeTemp: f64,
    pub mBrakePressure: f64,
    pub mRotation: f64,
    pub mLateralPatchVel: f64,
    pub mLongitudinalPatchVel: f64,
    pub mLateralGroundVel: f64,
    pub mLongitudinalGroundVel: f64,
    pub mCamber: f64,
    pub mLateralForce: f64,
    pub mLongitudinalForce: f64,
    pub mTireLoad: f64,
    pub mGripFract: f64,
    pub mPressure: f64,
    /// Kelvin, gauche / centre / droite.
    pub mTemperature: [f64; 3],
    pub mWear: f64,
    pub mTerrainName: [u8; 16],
    /// 0 sec, 1 mouillé, 2 herbe, 3 terre, 4 gravier, 5 vibreur, 6 spécial.
    pub mSurfaceType: u8,
    pub mFlat: u8,
    pub mDetached: u8,
    pub mStaticUndeflectedRadius: u8,
    pub mVerticalTireDeflection: f64,
    pub mWheelYLocation: f64,
    pub mToe: f64,
    pub mTireCarcassTemperature: f64,
    pub mTireInnerLayerTemperature: [f64; 3],
    pub mOptimalTemp: f32,
    pub mCompoundIndex: u8,
    pub mCompoundType: u8,
    pub mExpansion: [u8; 18],
}
assert_size!(Wheel, 260);

/// `TelemInfoV01` : télémétrie d'une voiture (≈100 Hz).
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct VehicleTelemetry {
    pub mID: i32,
    pub mDeltaTime: f64,
    /// Temps de session (s).
    pub mElapsedTime: f64,
    pub mLapNumber: i32,
    /// Temps de session au début du tour en cours.
    pub mLapStartET: f64,
    pub mVehicleName: [u8; 64],
    pub mTrackName: [u8; 64],
    /// Position dans le monde (m) : y vers le haut.
    pub mPos: Vec3,
    pub mLocalVel: Vec3,
    pub mLocalAccel: Vec3,
    pub mOri: [Vec3; 3],
    pub mLocalRot: Vec3,
    pub mLocalRotAccel: Vec3,
    pub mGear: i32,
    pub mEngineRPM: f64,
    pub mEngineWaterTemp: f64,
    pub mEngineOilTemp: f64,
    pub mClutchRPM: f64,
    pub mUnfilteredThrottle: f64,
    pub mUnfilteredBrake: f64,
    /// −1 (gauche) … +1 (droite).
    pub mUnfilteredSteering: f64,
    pub mUnfilteredClutch: f64,
    pub mFilteredThrottle: f64,
    pub mFilteredBrake: f64,
    pub mFilteredSteering: f64,
    pub mFilteredClutch: f64,
    pub mSteeringShaftTorque: f64,
    pub mFront3rdDeflection: f64,
    pub mRear3rdDeflection: f64,
    pub mFrontWingHeight: f64,
    pub mFrontRideHeight: f64,
    pub mRearRideHeight: f64,
    pub mDrag: f64,
    pub mFrontDownforce: f64,
    pub mRearDownforce: f64,
    /// Litres.
    pub mFuel: f64,
    pub mEngineMaxRPM: f64,
    pub mScheduledStops: u8,
    pub mOverheating: u8,
    pub mDetached: u8,
    pub mHeadlights: u8,
    pub mDentSeverity: [u8; 8],
    pub mLastImpactET: f64,
    pub mLastImpactMagnitude: f64,
    pub mLastImpactPos: Vec3,
    pub mEngineTorque: f64,
    /// Secteur en cours (0..2), voie des stands dans le bit de signe.
    pub mCurrentSector: i32,
    pub mSpeedLimiter: u8,
    pub mMaxGears: u8,
    pub mFrontTireCompoundIndex: u8,
    pub mRearTireCompoundIndex: u8,
    pub mFuelCapacity: f64,
    pub mFrontFlapActivated: u8,
    pub mRearFlapActivated: u8,
    pub mRearFlapLegalStatus: u8,
    pub mIgnitionStarter: u8,
    pub mFrontTireCompoundName: [u8; 18],
    pub mRearTireCompoundName: [u8; 18],
    pub mSpeedLimiterAvailable: u8,
    pub mAntiStallActivated: u8,
    pub mUnused: [u8; 2],
    pub mVisualSteeringWheelRange: f32,
    pub mRearBrakeBias: f64,
    pub mTurboBoostPressure: f64,
    pub mPhysicsToGraphicsOffset: [f32; 3],
    /// Débattement physique du volant (degrés, butée à butée).
    pub mPhysicalSteeringWheelRange: f32,
    pub mDeltaBest: f64,
    pub mBatteryChargeFraction: f64,
    pub mElectricBoostMotorTorque: f64,
    pub mElectricBoostMotorRPM: f64,
    pub mElectricBoostMotorTemperature: f64,
    pub mElectricBoostWaterTemperature: f64,
    pub mElectricBoostMotorState: u8,
    /// Tour invalidé (limites de piste…).
    pub mLapInvalidated: u8,
    pub mABSActive: u8,
    pub mTCActive: u8,
    pub mSpeedLimiterActive: u8,
    pub mWiperState: u8,
    pub mTC: u8,
    pub mTCMax: u8,
    pub mTCSlip: u8,
    pub mTCSlipMax: u8,
    pub mTCCut: u8,
    pub mTCCutMax: u8,
    pub mABS: u8,
    pub mABSMax: u8,
    pub mMotorMap: u8,
    pub mMotorMapMax: u8,
    pub mMigration: u8,
    pub mMigrationMax: u8,
    pub mFrontAntiSway: u8,
    pub mFrontAntiSwayMax: u8,
    pub mRearAntiSway: u8,
    pub mRearAntiSwayMax: u8,
    pub mLiftAndCoastProgress: u8,
    pub mTrackLimitsSteps: u8,
    pub mRegen: f32,
    pub mStateOfCharge: f32,
    pub mVirtualEnergy: f32,
    pub mTimeGapCarAhead: f32,
    pub mTimeGapCarBehind: f32,
    pub mTimeGapPlaceAhead: f32,
    pub mTimeGapPlaceBehind: f32,
    /// Marque et modèle.
    pub mVehicleModel: [u8; 30],
    pub mVehicleClass: u8,
    pub mVehicleChampionship: u8,
    pub mExpansion: [u8; 20],
    /// Avant gauche, avant droite, arrière gauche, arrière droite.
    pub mWheels: [Wheel; 4],
}
assert_size!(VehicleTelemetry, 1888);

/// `VehicleScoringInfoV01` : classement d'une voiture (≈5 Hz).
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct VehicleScoring {
    pub mID: i32,
    pub mDriverName: [u8; 32],
    pub mVehicleName: [u8; 64],
    /// Tours bouclés.
    pub mTotalLaps: i16,
    pub mSector: i8,
    pub mFinishStatus: i8,
    /// Distance depuis la ligne (m).
    pub mLapDist: f64,
    pub mPathLateral: f64,
    pub mTrackEdge: f64,
    pub mBestSector1: f64,
    pub mBestSector2: f64,
    pub mBestLapTime: f64,
    /// Secteur 1, puis secteurs 1+2, du dernier tour.
    pub mLastSector1: f64,
    pub mLastSector2: f64,
    pub mLastLapTime: f64,
    pub mCurSector1: f64,
    pub mCurSector2: f64,
    pub mNumPitstops: i16,
    pub mNumPenalties: i16,
    pub mIsPlayer: u8,
    pub mControl: i8,
    pub mInPits: u8,
    /// Position (1 = en tête).
    pub mPlace: u8,
    pub mVehicleClass: [u8; 32],
    pub mTimeBehindNext: f64,
    pub mLapsBehindNext: i32,
    pub mTimeBehindLeader: f64,
    pub mLapsBehindLeader: i32,
    pub mLapStartET: f64,
    pub mPos: Vec3,
    pub mLocalVel: Vec3,
    pub mLocalAccel: Vec3,
    pub mOri: [Vec3; 3],
    pub mLocalRot: Vec3,
    pub mLocalRotAccel: Vec3,
    pub mHeadlights: u8,
    pub mPitState: u8,
    pub mServerScored: u8,
    pub mIndividualPhase: u8,
    /// Position en qualification (1 = pole), −1 si inconnue.
    pub mQualification: i32,
    pub mTimeIntoLap: f64,
    pub mEstimatedLapTime: f64,
    pub mPitGroup: [u8; 24],
    pub mFlag: u8,
    pub mUnderYellow: u8,
    pub mCountLapFlag: u8,
    pub mInGarageStall: u8,
    pub mUpgradePack: [u8; 16],
    pub mPitLapDist: f32,
    pub mBestLapSector1: f32,
    pub mBestLapSector2: f32,
    pub mSteamID: u64,
    pub mVehFilename: [u8; 32],
    pub mAttackMode: i16,
    pub mFuelFraction: u8,
    pub mDRSState: u8,
    pub mExpansion: [u8; 4],
}
assert_size!(VehicleScoring, 584);

/// `ScoringInfoV01` : informations de séance.
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct ScoringInfo {
    pub mTrackName: [u8; 64],
    /// 0 journée test, 1-4 essais, 5-8 qualifications, 9 warm-up, 10-13 course.
    pub mSession: i32,
    pub mCurrentET: f64,
    pub mEndET: f64,
    pub mMaxLaps: i32,
    /// Longueur du circuit (m).
    pub mLapDist: f64,
    pub mResultsStreamPointer: [u8; 8],
    pub mNumVehicles: i32,
    /// 5 = drapeau vert, 6 = neutralisation (FCY / voiture de sécurité)…
    pub mGamePhase: u8,
    pub mYellowFlagState: i8,
    pub mSectorFlag: [u8; 3],
    pub mStartLight: u8,
    pub mNumRedLights: u8,
    pub mInRealtime: u8,
    pub mPlayerName: [u8; 32],
    pub mPlrFileName: [u8; 64],
    pub mDarkCloud: f64,
    pub mRaining: f64,
    pub mAmbientTemp: f64,
    pub mTrackTemp: f64,
    pub mWind: Vec3,
    pub mMinPathWetness: f64,
    pub mMaxPathWetness: f64,
    pub mGameMode: u8,
    pub mIsPasswordProtected: u8,
    pub mServerPort: u16,
    pub mServerPublicIP: u32,
    pub mMaxPlayers: i32,
    pub mServerName: [u8; 32],
    pub mStartET: f32,
    pub mAvgPathWetness: f64,
    pub mSessionTimeRemaining: f32,
    pub mTimeOfDay: f32,
    pub mIsFixedSetup: u8,
    pub mTrackGripLevel: u8,
    pub mCloudCoverage: u8,
    pub mTrackLimitsStepsPerPenalty: u8,
    pub mTrackLimitsStepsPerPoint: u8,
    pub mExpansion: [u8; 187],
    pub mVehiclePointer: [u8; 8],
}
assert_size!(ScoringInfo, 548);

// --- Conteneurs (SharedMemoryInterface.hpp : SharedMemoryObjectOut) -----------------------------

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct ApplicationState {
    pub mAppWindow: u64,
    pub mWidth: u32,
    pub mHeight: u32,
    pub mRefreshRate: u32,
    pub mWindowed: u32,
    pub mOptionsLocation: u8,
    pub mOptionsPage: [u8; 31],
    pub mExpansion: [u8; 204],
}
assert_size!(ApplicationState, 260);

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct Generic {
    /// Compteurs d'événements (SME_ENTER … SME_FFB).
    pub events: [u32; 16],
    /// Numéro de build (14000 = 1.4).
    pub gameVersion: i32,
    pub FFBTorque: f32,
    pub appInfo: ApplicationState,
}
assert_size!(Generic, 332);

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct PathData {
    pub userData: [u8; 260],
    pub customVariables: [u8; 260],
    pub stewardResults: [u8; 260],
    pub playerProfile: [u8; 260],
    pub pluginsFolder: [u8; 260],
}
assert_size!(PathData, 1300);

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct ScoringData {
    pub scoringInfo: ScoringInfo,
    pub scoringStreamSize: [u8; 12],
    pub vehScoringInfo: [VehicleScoring; MAX_MAPPED_VEHICLES],
    pub scoringStream: [u8; 65536],
}
assert_size!(ScoringData, 126_832);

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct TelemetryData {
    pub activeVehicles: u8,
    pub playerVehicleIdx: u8,
    pub playerHasVehicle: u8,
    pub telemInfo: [VehicleTelemetry; MAX_MAPPED_VEHICLES],
}
assert_size!(TelemetryData, 196_356);

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct ObjectOut {
    pub generic: Generic,
    pub paths: PathData,
    pub scoring: ScoringData,
    pub telemetry: TelemetryData,
}
/// Taille totale de la projection.
pub const LMU_DATA_SIZE: usize = 324_820;
assert_size!(ObjectOut, LMU_DATA_SIZE);

use std::mem::{offset_of, size_of};

pub const OFF_GAME_VERSION: usize = offset_of!(ObjectOut, generic) + offset_of!(Generic, gameVersion);
pub const OFF_SCORING_INFO: usize = offset_of!(ObjectOut, scoring) + offset_of!(ScoringData, scoringInfo);
pub const OFF_SCORING_ET: usize = OFF_SCORING_INFO + offset_of!(ScoringInfo, mCurrentET);
pub const OFF_NUM_VEHICLES: usize = OFF_SCORING_INFO + offset_of!(ScoringInfo, mNumVehicles);
pub const OFF_VEH_SCORING: usize = offset_of!(ObjectOut, scoring) + offset_of!(ScoringData, vehScoringInfo);
pub const OFF_ACTIVE_VEHICLES: usize = offset_of!(ObjectOut, telemetry) + offset_of!(TelemetryData, activeVehicles);
pub const OFF_PLAYER_IDX: usize = offset_of!(ObjectOut, telemetry) + offset_of!(TelemetryData, playerVehicleIdx);
pub const OFF_PLAYER_HAS_VEHICLE: usize = offset_of!(ObjectOut, telemetry) + offset_of!(TelemetryData, playerHasVehicle);
pub const OFF_TELEM_INFO: usize = offset_of!(ObjectOut, telemetry) + offset_of!(TelemetryData, telemInfo);
const OFF_VEH_ELAPSED: usize = offset_of!(VehicleTelemetry, mElapsedTime);
const OFF_IS_PLAYER: usize = offset_of!(VehicleScoring, mIsPlayer);

/// Copie cohérente des données utiles au pilote : séance, sa ligne de classement, sa télémétrie.
#[derive(Clone, Copy)]
pub struct Frame {
    pub game_version: i32,
    pub info: ScoringInfo,
    pub scoring: Option<VehicleScoring>,
    pub telem: Option<VehicleTelemetry>,
}

/// Texte C (terminé par 0) → `String`.
pub fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let raw = &b[..end];
    match std::str::from_utf8(raw) {
        Ok(s) => s.trim().to_string(),
        // Le jeu écrit en Windows-1252 : on conserve les caractères Latin-1, qui en sont l'essentiel.
        Err(_) => raw.iter().map(|&c| c as char).collect::<String>().trim().to_string(),
    }
}

/// Lit une image cohérente depuis le début de la projection `LMU_Data`.
///
/// # Safety
/// `base` doit pointer sur au moins `LMU_DATA_SIZE` octets lisibles.
pub unsafe fn read_frame_ptr(base: *const u8) -> Option<Frame> {
    unsafe {
        let at = |off: usize| base.add(off);
        let rd_u8 = |off: usize| std::ptr::read_volatile(at(off));
        let rd_f64_bits = |off: usize| std::ptr::read_unaligned(at(off) as *const u64);
        for _ in 0..5 {
            let has = rd_u8(OFF_PLAYER_HAS_VEHICLE) != 0;
            let idx = rd_u8(OFF_PLAYER_IDX) as usize;
            let active = rd_u8(OFF_ACTIVE_VEHICLES) as usize;
            let telem_off = (has && idx < MAX_MAPPED_VEHICLES && idx < active.max(1)).then(|| OFF_TELEM_INFO + idx * size_of::<VehicleTelemetry>());
            let witness = |t: Option<usize>| (rd_f64_bits(OFF_SCORING_ET), t.map(|o| rd_f64_bits(o + OFF_VEH_ELAPSED)));
            let before = witness(telem_off);
            let info: ScoringInfo = std::ptr::read_unaligned(at(OFF_SCORING_INFO) as *const ScoringInfo);
            let n = (info.mNumVehicles.max(0) as usize).min(MAX_MAPPED_VEHICLES);
            let scoring = (0..n)
                .map(|i| OFF_VEH_SCORING + i * size_of::<VehicleScoring>())
                .find(|o| rd_u8(o + OFF_IS_PLAYER) != 0)
                .map(|o| std::ptr::read_unaligned(at(o) as *const VehicleScoring));
            let telem = telem_off.map(|o| std::ptr::read_unaligned(at(o) as *const VehicleTelemetry));
            if witness(telem_off) != before {
                std::hint::spin_loop();
                continue;
            }
            let game_version = std::ptr::read_unaligned(at(OFF_GAME_VERSION) as *const i32);
            return Some(Frame { game_version, info, scoring, telem });
        }
        None
    }
}

/// Lecture depuis une copie en mémoire (tests, enregistrements bruts).
pub fn read_frame(mem: &[u8]) -> Option<Frame> {
    if mem.len() < LMU_DATA_SIZE {
        return None;
    }
    // SAFETY : la tranche couvre toute la projection.
    unsafe { read_frame_ptr(mem.as_ptr()) }
}

/// Écriture d'une valeur dans une copie (tests).
#[cfg(test)]
pub fn put<T: Copy>(mem: &mut [u8], off: usize, v: T) {
    assert!(off + size_of::<T>() <= mem.len());
    unsafe { std::ptr::write_unaligned(mem.as_mut_ptr().add(off) as *mut T, v) }
}

/// Valeur entièrement à zéro (structures de données brutes : tout motif de bits est valide).
pub fn zeroed<T: Copy>() -> T {
    // SAFETY : réservé aux structures `repr(C)` de ce module, faites d'entiers, de flottants et d'octets.
    unsafe { std::mem::zeroed() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_match_the_published_layout() {
        // Valeurs recoupées avec lmu-pitwall (vérifié sur LMU 1.4).
        assert_eq!(OFF_SCORING_INFO, 332 + 1300);
        assert_eq!(OFF_VEH_SCORING, OFF_SCORING_INFO + 548 + 12);
        assert_eq!(OFF_ACTIVE_VEHICLES, 332 + 1300 + 126_832);
        assert_eq!(OFF_PLAYER_IDX, OFF_ACTIVE_VEHICLES + 1);
        assert_eq!(OFF_TELEM_INFO, OFF_ACTIVE_VEHICLES + 4);
        assert_eq!(OFF_TELEM_INFO + 1888 * MAX_MAPPED_VEHICLES, LMU_DATA_SIZE);
    }

    #[test]
    fn reads_player_rows_from_a_buffer() {
        let mut mem = vec![0u8; LMU_DATA_SIZE];
        let mut info: ScoringInfo = zeroed();
        info.mTrackName[..9].copy_from_slice(b"Le Mans 1");
        info.mNumVehicles = 3;
        info.mLapDist = 13626.0;
        info.mSession = 10;
        put(&mut mem, OFF_SCORING_INFO, info);
        let mut me: VehicleScoring = zeroed();
        me.mIsPlayer = 1;
        me.mPlace = 4;
        me.mLapDist = 1234.5;
        put(&mut mem, OFF_VEH_SCORING + 2 * size_of::<VehicleScoring>(), me);
        let mut t: VehicleTelemetry = zeroed();
        t.mElapsedTime = 42.0;
        t.mFuel = 61.5;
        t.mWheels[3].mSurfaceType = 5;
        put(&mut mem, OFF_TELEM_INFO + 2 * size_of::<VehicleTelemetry>(), t);
        put(&mut mem, OFF_ACTIVE_VEHICLES, 3u8);
        put(&mut mem, OFF_PLAYER_IDX, 2u8);
        put(&mut mem, OFF_PLAYER_HAS_VEHICLE, 1u8);
        put(&mut mem, OFF_GAME_VERSION, 14000i32);

        let f = read_frame(&mem).unwrap();
        assert_eq!(f.game_version, 14000);
        assert_eq!(cstr(&f.info.mTrackName), "Le Mans 1");
        let s = f.scoring.unwrap();
        assert_eq!({ s.mPlace }, 4);
        assert_eq!({ s.mLapDist }, 1234.5);
        let t = f.telem.unwrap();
        assert_eq!({ t.mFuel }, 61.5);
        assert_eq!({ t.mWheels[3].mSurfaceType }, 5);
    }

    #[test]
    fn no_player_car_gives_no_rows() {
        let mem = vec![0u8; LMU_DATA_SIZE];
        let f = read_frame(&mem).unwrap();
        assert!(f.scoring.is_none() && f.telem.is_none());
    }

    #[test]
    fn latin1_names_survive() {
        assert_eq!(cstr(b"Portim\xe3o\0zz"), "Portimão");
    }
}
