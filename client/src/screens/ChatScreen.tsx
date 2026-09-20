// Preview only — chat/RAG over captures wasn't in the original V1 backend
// scope (see the plan doc); this was added by the design session and needs
// its own decision on a backend endpoint before it can do anything real.
export function ChatScreen() {
  return (
    <>
      <p className="dim">Preview only — there's no chat/RAG endpoint on the backend yet.</p>
      <div className="chat-thread">
        <div className="chat-bubble chat-bubble-user">What did I last say about the new office?</div>
        <div className="chat-bubble-assistant-wrap">
          <div className="dim chat-derived-label">[AI-derived]</div>
          <div className="panel chat-bubble">
            On September 16 you mentioned the lease for a new office downtown should arrive that
            week. Before that, on September 9, you mentioned a broken chair at your current desk.
          </div>
          <div className="chat-sources">
            <span className="dim">[from capture · Sep 16]</span>
            <span className="dim">[from capture · Sep 9]</span>
          </div>
        </div>
      </div>
      <div className="search-box chat-input">
        <span className="kicker">&gt;</span>
        <span className="dim">Ask something about your captures…</span>
        <span className="dim chat-send">[ Send ]</span>
      </div>
    </>
  );
}
