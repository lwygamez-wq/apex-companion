//! Débrief par IA (« ingénieur de piste ») : conversation en langage naturel sur une séance.
//!
//! Principe : l'IA ne voit jamais la télémétrie brute. Apex lui envoie la synthèse chiffrée qu'il a
//! calculée (tours, écarts par virage, conseils, carte de risque, régularité, course) et une consigne
//! stricte : répondre uniquement à partir de ces chiffres, sans en inventer. Elle reformule, relie et
//! explique ; les mesures restent celles d'Apex.
//!
//! Service au choix du pilote : API Claude (payante à l'usage), ou un fournisseur au format « OpenAI »
//! (Gemini, Mistral, Groq : offres gratuites ; Ollama : IA locale sur le PC, hors ligne).

use serde::{Deserialize, Serialize};
use serde_json::json;

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// Taille maximale de la conversation conservée (messages), pour borner le coût d'une question.
const MAX_TURNS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// « user » ou « assistant ».
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatReply {
    pub text: String,
    /// Jetons en entrée facturés plein tarif (dont écriture en cache), lus depuis le cache, et en sortie.
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    /// Coût indicatif de la réponse (USD).
    pub cost_usd: f64,
}

/// Consignes de l'ingénieur de piste.
pub fn system_prompt(beginner: bool) -> String {
    let mut s = String::from(
        "Tu es l'ingénieur de piste d'un pilote de simracing, dans l'application Apex Companion. \
Tu réponds en français, tutoiement, ton direct et bienveillant, comme à la radio d'un stand.\n\n\
Règles absolues :\n\
- Tu t'appuies UNIQUEMENT sur les données de séance fournies (bloc DONNÉES). Elles sont mesurées par Apex : ce sont les seuls faits.\n\
- Tu n'inventes jamais un chiffre, un virage, un point de freinage ou une cause. Si l'information n'est pas dans les données, dis-le simplement (« je n'ai pas cette mesure »).\n\
- Quand tu cites un chiffre, reprends-le tel quel depuis les données.\n\
- Les conseils de technique doivent découler des écarts mesurés (freinage, vitesse mini, remise des gaz, vitesse de sortie, sorties de piste). Tu peux expliquer le pourquoi en termes de pilotage général, sans affirmer ce que les données ne montrent pas.\n\
- Pas de conseils de réglages (setup) : tu n'as pas ces données.\n\
- Réponses courtes : 3 à 8 lignes, une priorité claire. Pas de tableau sauf si on te le demande.\n",
    );
    if beginner {
        s += "- Le pilote est débutant : vocabulaire simple, une seule chose à travailler à la fois, la régularité et rester sur la piste avant la vitesse pure.\n";
    }
    s
}

/// Message envoyé à l'API : données de séance en tête de la première question, puis l'échange.
pub fn build_request(model: &str, system: &str, context: &str, history: &[ChatMessage]) -> serde_json::Value {
    let start = history.len().saturating_sub(MAX_TURNS);
    let mut msgs: Vec<serde_json::Value> = vec![];
    for (i, m) in history[start..].iter().enumerate() {
        let role = if m.role == "assistant" { "assistant" } else { "user" };
        if i == 0 && role == "user" {
            // Données mises en cache côté API : les questions suivantes de la conversation coûtent ~10 fois moins.
            msgs.push(json!({ "role": "user", "content": [
                { "type": "text", "text": format!("DONNÉES DE LA SÉANCE (mesurées par Apex) :\n{context}"), "cache_control": { "type": "ephemeral" } },
                { "type": "text", "text": m.text },
            ]}));
        } else {
            msgs.push(json!({ "role": role, "content": m.text }));
        }
    }
    // L'API exige un premier message « user » : une conversation tronquée qui commencerait par
    // l'assistant est complétée par les données seules.
    if msgs.first().is_some_and(|m| m["role"] == "assistant") {
        msgs.insert(0, json!({ "role": "user", "content": format!("DONNÉES DE LA SÉANCE (mesurées par Apex) :\n{context}") }));
    }
    json!({ "model": model, "max_tokens": 1500, "system": system, "messages": msgs })
}

/// Texte de la réponse (blocs « text » seulement ; la réflexion éventuelle est ignorée).
pub fn parse_reply(v: &serde_json::Value) -> Result<ChatReply, String> {
    if let Some(e) = v.get("error") {
        return Err(e["message"].as_str().unwrap_or("erreur de l'API").to_string());
    }
    let text: String = v["content"]
        .as_array()
        .ok_or("réponse de l'IA illisible")?
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err("réponse vide de l'IA".into());
    }
    let u = &v["usage"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    let (input, cached, output) = (n("input_tokens") + n("cache_creation_input_tokens"), n("cache_read_input_tokens"), n("output_tokens"));
    Ok(ChatReply { text, input_tokens: input, cached_tokens: cached, output_tokens: output, cost_usd: approx_cost_usd(input, cached, output) })
}

/// Fournisseur du débrief.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Gemini,
    Mistral,
    Groq,
    Ollama,
}

impl Provider {
    pub fn parse(s: &str) -> Provider {
        match s {
            "gemini" => Provider::Gemini,
            "mistral" => Provider::Mistral,
            "groq" => Provider::Groq,
            "ollama" => Provider::Ollama,
            _ => Provider::Anthropic,
        }
    }
    /// Adresse « chat completions » (format OpenAI) ; Claude a la sienne.
    pub fn url(self) -> &'static str {
        match self {
            Provider::Anthropic => API_URL,
            Provider::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
            Provider::Mistral => "https://api.mistral.ai/v1/chat/completions",
            Provider::Groq => "https://api.groq.com/openai/v1/chat/completions",
            Provider::Ollama => "http://127.0.0.1:11434/v1/chat/completions",
        }
    }
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => DEFAULT_MODEL,
            Provider::Gemini => "gemini-3.8-flash",
            Provider::Mistral => "mistral-small-latest",
            Provider::Groq => "llama-3.3-70b-versatile",
            Provider::Ollama => "mistral",
        }
    }
    pub fn needs_key(self) -> bool {
        self != Provider::Ollama
    }
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "Claude",
            Provider::Gemini => "Gemini",
            Provider::Mistral => "Mistral",
            Provider::Groq => "Groq",
            Provider::Ollama => "Ollama",
        }
    }
}

/// Même échange au format « OpenAI » : consigne en message système, données en tête de la première question.
pub fn build_openai_request(model: &str, system: &str, context: &str, history: &[ChatMessage]) -> serde_json::Value {
    let start = history.len().saturating_sub(MAX_TURNS);
    let mut msgs = vec![json!({ "role": "system", "content": system })];
    for (i, m) in history[start..].iter().enumerate() {
        let role = if m.role == "assistant" { "assistant" } else { "user" };
        if i == 0 {
            let first = if role == "user" { format!("DONNÉES DE LA SÉANCE (mesurées par Apex) :\n{context}\n\n{}", m.text) } else { format!("DONNÉES DE LA SÉANCE (mesurées par Apex) :\n{context}") };
            msgs.push(json!({ "role": "user", "content": first }));
            if role == "assistant" {
                msgs.push(json!({ "role": "assistant", "content": m.text }));
            }
        } else {
            msgs.push(json!({ "role": role, "content": m.text }));
        }
    }
    // Marge large : chez certains fournisseurs (Gemini « thinking »), la réflexion compte dans ce plafond.
    json!({ "model": model, "max_tokens": 4000, "messages": msgs })
}

const EMPTY: &str = "réponse vide de l'IA : réessaie, ou choisis un autre modèle";

/// Message d'erreur d'une réponse au format OpenAI (Gemini renvoie parfois une liste).
fn openai_error(v: &serde_json::Value) -> Option<String> {
    let e = if v.is_array() { &v[0]["error"] } else { &v["error"] };
    e["message"].as_str().map(str::to_string).or_else(|| e.as_str().map(str::to_string))
}

pub fn parse_openai_reply(v: &serde_json::Value) -> Result<ChatReply, String> {
    if let Some(e) = openai_error(v) {
        return Err(e);
    }
    let text = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Err(EMPTY.into());
    }
    let u = &v["usage"];
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    // Offres gratuites ou locales : pas de coût affiché.
    Ok(ChatReply { text, input_tokens: n("prompt_tokens"), cached_tokens: 0, output_tokens: n("completion_tokens"), cost_usd: 0.0 })
}

/// Appel d'un fournisseur au format OpenAI.
pub fn ask_openai(provider: Provider, api_key: Option<&str>, body: &serde_json::Value) -> Result<ChatReply, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(120))).http_status_as_error(false).build().into();
    let mut req = agent.post(provider.url()).header("content-type", "application/json");
    if let Some(k) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
        req = req.header("authorization", &format!("Bearer {k}"));
    }
    let mut resp = req.send_json(body).map_err(|e| {
        if provider == Provider::Ollama {
            format!("Ollama ne répond pas sur ce PC : installe-le (ollama.com), lance-le, puis télécharge le modèle (« ollama pull {} »). ({e})", body["model"].as_str().unwrap_or("mistral"))
        } else {
            format!("connexion à {} impossible ({e})", provider.name())
        }
    })?;
    let status = resp.status().as_u16();
    let v: serde_json::Value = resp.body_mut().read_json().unwrap_or(serde_json::Value::Null);
    match status {
        200 => parse_openai_reply(&v),
        401 | 403 => Err(format!("clé refusée par {} : vérifie-la dans Réglages → Comptes & clés", provider.name())),
        404 => Err(format!(
            "modèle « {} » introuvable chez {} : clique sur « Tester », Apex choisira un modèle disponible{}",
            body["model"].as_str().unwrap_or("?"),
            provider.name(),
            openai_error(&v).map(|m| format!(" ({m})")).unwrap_or_default()
        )),
        429 => Err(format!("limite de l'offre gratuite {} atteinte : réessaie dans une minute (ou demain si c'est la limite du jour)", provider.name())),
        _ => Err(openai_error(&v).map(|m| format!("{} ({status}) : {m}", provider.name())).unwrap_or_else(|| format!("{} : erreur {status}", provider.name()))),
    }
}

/// Débrief avec le fournisseur choisi.
pub fn chat(provider: Provider, api_key: Option<&str>, model: &str, system: &str, context: &str, history: &[ChatMessage]) -> Result<ChatReply, String> {
    match provider {
        Provider::Anthropic => ask(api_key.unwrap_or(""), &build_request(model, system, context, history)),
        p => ask_openai(p, api_key, &build_openai_request(model, system, context, history)),
    }
}

/// Appel de l'API Claude.
pub fn ask(api_key: &str, body: &serde_json::Value) -> Result<ChatReply, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(90))).http_status_as_error(false).build().into();
    let mut resp = agent
        .post(API_URL)
        .header("x-api-key", api_key.trim())
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .send_json(body)
        .map_err(|e| format!("connexion à l'API impossible ({e})"))?;
    let status = resp.status().as_u16();
    let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| format!("réponse illisible ({e})"))?;
    match status {
        200 => parse_reply(&v),
        401 => Err("clé API refusée : vérifie-la dans Réglages → Débrief IA".into()),
        429 => Err("trop de demandes ou crédit épuisé sur ton compte API : réessaie dans un moment".into()),
        400 if v["error"]["message"].as_str().is_some_and(|m| m.contains("credit balance")) => {
            Err("clé valide, mais pas de crédit sur ce compte API Claude (facturation séparée de l'abonnement Pro). Recharge sur console.anthropic.com, ou choisis un fournisseur gratuit (Gemini, Mistral…) dans Réglages → Comptes & clés.".into())
        }
        _ => Err(v["error"]["message"].as_str().map(|m| format!("API ({status}) : {m}")).unwrap_or_else(|| format!("API : erreur {status}"))),
    }
}

/// Essai de la clé : une question minuscule (quelques jetons, une fraction de centime) au modèle choisi.
/// Vérifie d'un coup la clé, le modèle et le crédit du compte.
pub fn check_request(model: &str) -> serde_json::Value {
    json!({ "model": model, "max_tokens": 16, "messages": [{ "role": "user", "content": "Réponds seulement : OK" }] })
}

/// Résultat de « Tester » : message, modèle retenu (corrigé s'il n'existe pas chez le fournisseur),
/// et la liste des modèles de chat disponibles avec cette clé.
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub message: String,
    pub model: String,
    pub models: Vec<String>,
}

/// Modèles de chat proposés par un fournisseur au format OpenAI (`GET …/models`), sans le préfixe
/// « models/ » de Gemini ni les modèles d'images, de voix ou d'intégration.
pub fn list_models(provider: Provider, api_key: &str) -> Result<Vec<String>, String> {
    if provider == Provider::Anthropic {
        return Ok(vec![]);
    }
    let url = provider.url().replace("/chat/completions", "/models");
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(20))).http_status_as_error(false).build().into();
    let mut req = agent.get(&url);
    if !api_key.trim().is_empty() {
        req = req.header("authorization", &format!("Bearer {}", api_key.trim()));
    }
    let mut resp = req.call().map_err(|e| format!("liste des modèles indisponible ({e})"))?;
    let status = resp.status().as_u16();
    let v: serde_json::Value = resp.body_mut().read_json().unwrap_or(serde_json::Value::Null);
    if status != 200 {
        return Err(openai_error(&v).unwrap_or_else(|| format!("liste des modèles : erreur {status}")));
    }
    Ok(chat_models(&v))
}

/// Identifiants des modèles de texte d'une réponse `/models`.
pub fn chat_models(v: &serde_json::Value) -> Vec<String> {
    const SKIP: [&str; 12] = ["embed", "image", "tts", "audio", "live", "vision", "whisper", "moderation", "guard", "veo", "imagen", "native"];
    let mut out: Vec<String> = v["data"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["id"].as_str()).map(|id| id.trim_start_matches("models/").to_string()).collect())
        .unwrap_or_default();
    out.retain(|id| !SKIP.iter().any(|s| id.to_lowercase().contains(s)));
    out.sort();
    out.dedup();
    out
}

/// Version d'un nom de modèle (« gemini-3.5-flash » → 3.5), pour prendre le plus récent.
fn version_of(id: &str) -> f32 {
    id.split(['-', '_']).find_map(|p| p.parse::<f32>().ok()).unwrap_or(0.0)
}

/// Meilleur modèle de débrief parmi ceux disponibles : un « Flash » Gemini récent et stable,
/// un « small » Mistral, un gros Llama chez Groq, le premier modèle installé chez Ollama.
pub fn pick_model(provider: Provider, models: &[String]) -> Option<String> {
    let stable = |m: &&String| !m.contains("preview") && !m.contains("exp");
    let best = |f: &dyn Fn(&String) -> bool| -> Option<String> {
        let mut c: Vec<&String> = models.iter().filter(|m| f(m)).collect();
        c.sort_by(|a, b| stable(b).cmp(&stable(a)).then(version_of(b).total_cmp(&version_of(a))).then(a.len().cmp(&b.len())));
        c.first().map(|s| s.to_string())
    };
    match provider {
        Provider::Gemini => best(&|m| m.starts_with("gemini") && m.contains("flash") && !m.contains("lite") && !m.contains("thinking")).or_else(|| best(&|m| m.starts_with("gemini"))),
        Provider::Mistral => models.iter().find(|m| *m == "mistral-small-latest").cloned().or_else(|| best(&|m| m.contains("small"))).or_else(|| models.first().cloned()),
        Provider::Groq => best(&|m| m.contains("llama") && m.contains("70b")).or_else(|| models.first().cloned()),
        Provider::Ollama => models.first().cloned(),
        Provider::Anthropic => None,
    }
}

/// Erreur « ce modèle n'existe pas / plus » (message d'Apex pour un 404, ou du fournisseur).
pub fn is_model_error(e: &str) -> bool {
    e.contains("introuvable chez") || e.contains("no longer available") || e.contains("not found for API version") || e.contains("is not found")
}

/// Modèle que le fournisseur conseille dans son message d'erreur, par exemple
/// « Please update your code to use models/gemini-3.8-flash ».
pub fn suggested_model(e: &str) -> Option<String> {
    let i = e.find("use models/")? + "use models/".len();
    let m: String = e[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_')).collect();
    let m = m.trim_end_matches('.').to_string();
    (!m.is_empty()).then_some(m)
}

pub fn check(provider: Provider, api_key: &str, model: &str) -> Result<CheckResult, String> {
    if provider.needs_key() && api_key.trim().is_empty() {
        return Err("aucune clé saisie".into());
    }
    let mut model = if model.trim().is_empty() { provider.default_model().to_string() } else { model.trim().to_string() };
    // Liste des modèles de la clé : le modèle saisi est remplacé s'il n'y figure pas (renommé, retiré).
    let models = list_models(provider, api_key).unwrap_or_default();
    let mut note = String::new();
    if !models.is_empty() && !models.contains(&model) {
        if let Some(m) = pick_model(provider, &models) {
            note = format!(" (« {model} » n'existe plus chez {} : remplacé par {m})", provider.name());
            model = m;
        }
    }
    if provider == Provider::Ollama && models.is_empty() {
        return Err("Ollama ne répond pas ou n'a aucun modèle : installe-le (ollama.com), lance-le, puis télécharge un modèle (« ollama pull mistral »).".into());
    }
    // Essai ; si le modèle est refusé (retiré « pour les nouveaux utilisateurs », renommé), on passe au
    // modèle que conseille le fournisseur dans son message, sinon au meilleur suivant de la liste.
    let mut tried: Vec<String> = vec![];
    let ok = loop {
        let r = match provider {
            Provider::Anthropic => ask(api_key, &check_request(&model)).map(|_| ()),
            // Réponse vide acceptée : le modèle a pu dépenser ses quelques jetons à réfléchir, la connexion est bonne.
            p => match ask_openai(p, Some(api_key), &json!({ "model": model, "max_tokens": 64, "messages": [{ "role": "user", "content": "Réponds seulement : OK" }] })) {
                Err(e) if e == EMPTY => Ok(()),
                r => r.map(|_| ()),
            },
        };
        match r {
            Err(e) if is_model_error(&e) && tried.len() < 4 => {
                tried.push(model.clone());
                let rest: Vec<String> = models.iter().filter(|m| !tried.contains(m)).cloned().collect();
                let Some(next) = suggested_model(&e).filter(|m| !tried.contains(m)).or_else(|| pick_model(provider, &rest)) else { break Err(e) };
                note = format!(" (« {} » n'est plus disponible chez {} : remplacé par {next})", tried[0], provider.name());
                model = next;
            }
            r => break r,
        }
    };
    ok.map(|_| CheckResult { message: format!("Connexion réussie : {} répond avec le modèle {model}{note}.", provider.name()), model, models })
}

/// Coût approximatif (USD), pour l'afficher au pilote. Tarif public de Claude Sonnet 5.5 (novembre 2026) :
/// 2 $ / million de jetons en entrée, 10 $ en sortie, lecture du cache à 5 % du prix d'entrée ; indicatif.
pub fn approx_cost_usd(input: u64, cached: u64, output: u64) -> f64 {
    input as f64 * 2.0 / 1e6 + cached as f64 * 0.1 / 1e6 + output as f64 * 10.0 / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(role: &str, t: &str) -> ChatMessage {
        ChatMessage { role: role.into(), text: t.into() }
    }

    #[test]
    fn data_ride_with_the_first_question_and_are_cached() {
        let b = build_request("claude-sonnet-5-5", "sys", "Tour 3 : 1:51.2", &[m("user", "Où je perds ?"), m("assistant", "Au V7."), m("user", "Pourquoi ?")]);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "user");
        assert!(msgs[0]["content"][0]["text"].as_str().unwrap().contains("Tour 3 : 1:51.2"));
        assert_eq!(msgs[0]["content"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(msgs[0]["content"][1]["text"], "Où je perds ?");
        assert_eq!(msgs[2]["content"], "Pourquoi ?");
        assert!(b.get("temperature").is_none(), "paramètre refusé par les modèles récents");
    }

    #[test]
    fn long_conversations_are_trimmed_but_keep_the_data() {
        let mut h = vec![];
        for i in 0..40 {
            h.push(m(if i % 2 == 0 { "user" } else { "assistant" }, &format!("msg {i}")));
        }
        h.push(m("user", "dernière"));
        let b = build_request("x", "s", "DATA", &h);
        let msgs = b["messages"].as_array().unwrap();
        assert!(msgs.len() <= MAX_TURNS + 1);
        assert_eq!(msgs[0]["role"], "user");
        assert!(serde_json::to_string(&msgs[0]).unwrap().contains("DATA"));
        assert_eq!(msgs.last().unwrap()["content"], "dernière");
    }

    #[test]
    fn openai_format_puts_the_data_in_the_first_question_and_reads_the_reply() {
        let b = build_openai_request("gemini-2.5-flash", "SYS", "Tour 3 : 1:51.2", &[m("user", "Où je perds ?"), m("assistant", "Au V7."), m("user", "Pourquoi ?")]);
        let msgs = b["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["role"], "system");
        assert!(msgs[1]["content"].as_str().unwrap().contains("Tour 3 : 1:51.2") && msgs[1]["content"].as_str().unwrap().ends_with("Où je perds ?"));
        assert_eq!(msgs[3]["content"], "Pourquoi ?");
        let r = parse_openai_reply(&json!({ "choices": [{ "message": { "content": " Freine plus tard. " } }], "usage": { "prompt_tokens": 900, "completion_tokens": 20 } })).unwrap();
        assert_eq!((r.text.as_str(), r.input_tokens, r.output_tokens, r.cost_usd), ("Freine plus tard.", 900, 20, 0.0));
        assert!(parse_openai_reply(&json!([{ "error": { "message": "API key not valid" } }])).unwrap_err().contains("not valid"));
        assert_eq!(Provider::parse("gemini"), Provider::Gemini);
        assert_eq!(Provider::parse("n'importe quoi"), Provider::Anthropic);
        assert!(!Provider::Ollama.needs_key());
    }

    #[test]
    fn key_check_is_a_tiny_request_and_refuses_an_empty_key() {
        let b = check_request("claude-sonnet-5-5");
        assert_eq!(b["max_tokens"], 16);
        assert_eq!(b["model"], "claude-sonnet-5-5");
        assert!(check(Provider::Anthropic, "  ", "x").unwrap_err().contains("aucune clé"));
        let list = chat_models(&json!({ "data": [
            { "id": "models/gemini-2.5-flash-lite" }, { "id": "models/gemini-3.5-flash" }, { "id": "models/gemini-3.8-flash-preview-05" },
            { "id": "models/gemini-3.6-flash" }, { "id": "models/gemini-3.8-flash-tts" }, { "id": "models/gemini-embedding-2" }, { "id": "models/gemini-2.5-pro" } ] }));
        assert!(!list.iter().any(|m| m.contains("tts") || m.contains("embedding")));
        assert_eq!(pick_model(Provider::Gemini, &list).as_deref(), Some("gemini-3.6-flash"));
        let e = "modèle « gemini-2.5-flash » introuvable chez Gemini : … (This model models/gemini-2.5-flash is no longer available to new users. Please update your code to use models/gemini-3.8-flash for the latest features and improvements.)";
        assert!(is_model_error(e));
        assert_eq!(suggested_model(e).as_deref(), Some("gemini-3.8-flash"));
        assert_eq!(suggested_model("autre erreur"), None);
        assert_eq!(pick_model(Provider::Mistral, &["mistral-large-latest".into(), "mistral-small-latest".into()]).as_deref(), Some("mistral-small-latest"));
        assert!(check(Provider::Gemini, "", "x").unwrap_err().contains("aucune clé"));
    }

    #[test]
    fn reply_text_ignores_thinking_and_counts_tokens() {
        let v = json!({ "content": [ { "type": "thinking", "thinking": "..." }, { "type": "text", "text": "Freine 10 m plus tard au V7." } ],
            "usage": { "input_tokens": 100, "cache_read_input_tokens": 9000, "output_tokens": 50 } });
        let r = parse_reply(&v).unwrap();
        assert_eq!(r.text, "Freine 10 m plus tard au V7.");
        assert_eq!((r.input_tokens, r.cached_tokens, r.output_tokens), (100, 9000, 50));
        assert!(r.cost_usd > 0.0 && r.cost_usd < 0.01);
        assert!(parse_reply(&json!({ "error": { "message": "invalid x-api-key" } })).unwrap_err().contains("invalid"));
    }
}
