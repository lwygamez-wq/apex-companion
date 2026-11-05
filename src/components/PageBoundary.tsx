import { Component, type ReactNode } from "react";

/**
 * Filet de sécurité : une page qui plante affiche l'erreur et un bouton de retour,
 * au lieu de laisser une fenêtre noire.
 */
export class PageBoundary extends Component<{ children: ReactNode; onHome: () => void }, { err: Error | null }> {
  state = { err: null as Error | null };
  static getDerivedStateFromError(err: Error) {
    return { err };
  }
  componentDidCatch(err: Error) {
    console.error("Page plantée :", err);
  }
  render() {
    if (!this.state.err) return this.props.children;
    return (
      <div className="card empty">
        <h2>Cette page a rencontré un problème</h2>
        <p className="muted">{this.state.err.message}</p>
        <p className="muted">Le reste de l'app fonctionne. Si ça se reproduit, Réglages › Avancé › « Copier le diagnostic » et envoie-le.</p>
        <button className="btn primary" onClick={() => (this.setState({ err: null }), this.props.onHome())}>Revenir au Briefing</button>
      </div>
    );
  }
}
