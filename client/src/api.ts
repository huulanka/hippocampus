// Mirrors the DTOs in the `contracts` Rust crate. Kept as plain types (not
// generated) since the surface is small; if it grows, revisit codegen.

import { apiAudio, apiRequest, getSettings, logError, runningInDesktopApp } from "./desktop";

export interface CaptureListItem {
  event_id: string;
  transcript_text: string;
  occurred_at: string;
  /// "audio" when a recording exists to play back, "text" when the capture
  /// was typed or dictated straight into the app.
  origin: "audio" | "text";
}

export interface EntitySummary {
  id: string;
  entity_type: string;
  name: string;
  current_summary: string | null;
}

/// An earlier capture surfaced as an echo. Always the verbatim transcript,
/// never a summary — see docs/adr/0006.
export interface EchoItem {
  capture_event_id: string;
  transcript_text: string;
  occurred_at: string;
  similarity: number;
  /// The cross-encoder's verdict, when one ran. A raw logit; deliberately
  /// never shown — it is here so thresholds can be tuned by looking.
  rerank_score: number | null;
}

export interface CaptureAccepted {
  event_id: string;
  occurred_at: string;
  echo: EchoItem[];
  /// The echo is still being judged; ask `getEcho` for it. Judging used
  /// to happen before this response was sent, which is what made saving a
  /// note take 28 seconds against the NAS.
  echo_pending: boolean;
}

/// A capture's echo, and whether it is final.
export interface EchoResponse {
  items: EchoItem[];
  /// Empty `items` with `pending` means "not yet". Empty without it means
  /// "nothing echoed" — a real and common answer.
  pending: boolean;
}

/// Everything known about one capture. Mirrors `contracts::CaptureDetail`;
/// the embedding is deliberately not part of it.
export interface CaptureDetail {
  event_id: string;
  occurred_at: string;
  origin: "audio" | "text";
  device: string;
  /// The transcript as it currently reads. `null` once redacted.
  text: string | null;
  redacted: boolean;
  audio: AudioDetail | null;
  transcripts: TranscriptVersion[];
  entities: EntityMention[];
  relations: RelationMention[];
  echo: EchoItem[];
  echo_pending: boolean;
  events: EventRecord[];
}

export interface AudioDetail {
  mime: string;
  duration_ms: number | null;
}

export interface TranscriptVersion {
  event_id: string;
  text: string;
  model: string;
  language: string | null;
  created_at: string;
  supersedes: string | null;
}

export interface EntityMention {
  id: string;
  entity_type: string;
  name: string;
  observation: string;
  model: string;
  confidence: number | null;
  /// The day this observation is about, when it is about one — "morgen"
  /// resolved against the moment the note was spoken. Not the same as
  /// when it was said.
  happened_on: string | null;
  /// Only set when a time of day was actually named.
  happened_at: string | null;
  /// "time" | "day" | "week" | "month" | "year"
  happened_precision: string | null;
}

export interface RelationMention {
  id: string;
  from_entity_id: string;
  from_name: string;
  to_entity_id: string;
  to_name: string;
  relation_type: string;
  model: string;
}

export interface EventRecord {
  id: string;
  event_type: string;
  source: string;
  occurred_at: string;
  payload: unknown;
}

export interface EntityListItem {
  id: string;
  entity_type: string;
  name: string;
  current_summary: string | null;
  mention_count: number;
  last_seen: string | null;
}

export interface EntityDetail {
  id: string;
  entity_type: string;
  name: string;
  current_summary: string | null;
  created_at: string;
  mentions: EntityCapture[];
  relations: EntityEdge[];
}

export interface EntityCapture {
  capture_event_id: string;
  transcript_text: string;
  occurred_at: string;
  observation: string;
  model: string;
  /// The day this observation is about, when it is about one — "morgen"
  /// resolved against the moment the note was spoken. Not the same as
  /// when it was said.
  happened_on: string | null;
  /// Only set when a time of day was actually named.
  happened_at: string | null;
  /// "time" | "day" | "week" | "month" | "year"
  happened_precision: string | null;
}

export interface EntityEdge {
  relation_type: string;
  /// False when this entity is the target of the relation.
  outgoing: boolean;
  other_id: string;
  other_name: string;
  other_type: string;
  /// The capture this edge was read out of. `null` when the
  /// consolidation run drew it across several captures — the only way an
  /// edge between two things that were never mentioned in one breath can
  /// exist.
  source_event_id: string | null;
}

export interface Resurfaced {
  upcoming: UpcomingItem[];
  threads: ThreadItem[];
}

export interface UpcomingItem {
  entity_id: string;
  entity_name: string;
  entity_type: string;
  observation: string;
  happened_on: string;
  happened_at: string | null;
  happened_precision: string | null;
  capture_event_id: string;
  /// When you said it — as opposed to when it is about.
  said_at: string;
}

export interface ThreadItem {
  entity_id: string;
  entity_type: string;
  name: string;
  current_summary: string | null;
  capture_count: number;
  first_seen: string;
  last_seen: string;
}

export interface SearchResult {
  capture_event_id: string;
  transcript_text: string;
  occurred_at: string;
  score: number;
  related_entities: EntitySummary[];
}

/// The entity graph in one piece. Mirrors `contracts::Graph`.
export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  /// Entities left out because the graph was capped. Shown as a number
  /// rather than hidden, so a partial picture never pretends to be whole.
  omitted_nodes: number;
}

export interface GraphNode {
  id: string;
  name: string;
  entity_type: string;
  mention_count: number;
  last_seen: string | null;
}

export interface GraphEdge {
  from: string;
  to: string;
  relation_type: string;
  /// How many separate captures assert this same relation.
  weight: number;
}

export interface EntityTypeCount {
  entity_type: string;
  count: number;
}

export interface SearchOptions {
  limit?: number;
  entityType?: string;
  from?: string;
  to?: string;
}

const DEFAULT_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "http://localhost:8080";

/// Which backend requests go to. Mutable so a value read from persisted
/// settings (desktop app) or set from the Settings screen can take effect
/// without a restart — see `initApiBaseUrl` and `setApiBaseUrl`.
let baseUrl = DEFAULT_BASE_URL;

export function getApiBaseUrl(): string {
  return baseUrl;
}

/// Points every subsequent request at a different backend. An empty value
/// falls back to `DEFAULT_BASE_URL`.
export function setApiBaseUrl(url: string | null | undefined): void {
  baseUrl = url && url.trim() ? url.trim() : DEFAULT_BASE_URL;
}

/// Loads the persisted backend URL, if the desktop app has one saved, so
/// this module can display it and the browser build can use it. Call once
/// at startup and await it before rendering — the browser build and a
/// fresh install both resolve to the plain default either way.
///
/// The Access credentials are deliberately not loaded here any more. In
/// the desktop app the request is built in Rust, which reads them itself;
/// the secret never crosses into the webview at all.
export async function initApiBaseUrl(): Promise<void> {
  const settings = await getSettings();
  setApiBaseUrl(settings.backend_url);
}

/// One request to the backend.
///
/// In the desktop app this goes through Rust: no origin, so no CORS
/// preflight, which is what made every request fail with
/// `TypeError: Load failed` the moment the backend moved behind
/// Cloudflare Access. In the browser build — development against a local
/// backend — there is no Rust side, so it stays a plain `fetch`, which is
/// fine because a local backend has no Access in front of it.
async function request<T>(path: string, init?: RequestInit): Promise<T> {
  if (runningInDesktopApp()) {
    let res: Awaited<ReturnType<typeof apiRequest>>;
    try {
      res = await apiRequest(
        init?.method ?? "GET",
        path,
        typeof init?.body === "string" ? init.body : undefined,
      );
    } catch (err) {
      const message = `${path} failed to reach ${baseUrl}: ${String(err)}`;
      logError(message);
      throw new Error(message);
    }
    if (res.status < 200 || res.status >= 300) {
      const message = `${path} failed: ${res.status} ${res.status_text}`;
      logError(message);
      throw new Error(message);
    }
    return JSON.parse(res.body) as T;
  }

  let res: Response;
  try {
    res = await fetch(`${baseUrl}${path}`, init);
  } catch (err) {
    const message = `${path} failed to reach ${baseUrl}: ${String(err)}`;
    logError(message);
    throw new Error(message);
  }
  if (!res.ok) {
    const message = `${path} failed: ${res.status} ${res.statusText}`;
    logError(message);
    throw new Error(message);
  }
  return (await res.json()) as T;
}

export function listCaptures(): Promise<CaptureListItem[]> {
  return request<CaptureListItem[]>("/captures");
}

/// Where a note came from, when it did not come from the capture field.
///
/// Carried as a label on the capture and nothing more. A session is
/// deliberately *not* an entity in the graph: the extraction finds
/// entities in what was said, and a heading typed into a text field is
/// not something that was said. Making "Workshop Datenqualität" a
/// node would mean the interface inventing a thing nobody spoke, which is
/// the exact line P12 draws.
export interface CaptureSession {
  id: string;
  title: string;
  started_at: string;
}

export interface CaptureOrigin {
  /// When the note was *written*, not when it was sent. Left out for a
  /// capture that is being sent as it is made.
  ///
  /// This is the whole reason a session can exist at all. Four hours of a
  /// workshop sent at the end would otherwise arrive carrying one
  /// timestamp, and every note in it would claim to have been thought at
  /// 14:00 — which makes the timeline, the only thing this app is really
  /// promising, start lying.
  occurredAt?: string;
  session?: CaptureSession;
}

/// Records a capture. The response carries its echoes inline, so the user
/// sees them without a second round trip.
export function createCapture(
  transcriptText: string,
  device: string,
  origin: CaptureOrigin = {},
): Promise<CaptureAccepted> {
  return request<CaptureAccepted>("/captures", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      transcript_text: transcriptText,
      device,
      // Where the note is being written, so the backend can turn
      // "morgen" into a real date rather than guessing.
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      occurred_at: origin.occurredAt,
      session: origin.session,
    }),
  });
}

/// One written note out of a session, and what became of it.
export interface SentNote {
  writtenAt: string;
  text: string;
  eventId: string | null;
  error: string | null;
}

/// Sends a whole session, one note at a time, each carrying the moment it
/// was written.
///
/// Sequential rather than parallel, and it does not stop at the first
/// failure. Both follow from what a session is: the notes are ordered and
/// a reader will read them in order, and after four hours of typing the
/// worst possible answer to a flaky connection is "none of it was kept".
/// Whatever got through is reported note by note, so the ones that did not
/// can be retried without sending the rest twice.
/// Two timestamps are the same moment if they agree to the second.
/// Postgres stores microseconds and the client sends milliseconds, so
/// insisting on an exact string match would fail on a backend that did
/// everything right.
function sameMoment(a: string, b: string): boolean {
  return Math.abs(new Date(a).getTime() - new Date(b).getTime()) < 1000;
}

export async function sendSession(
  notes: { text: string; writtenAt: string }[],
  session: CaptureSession,
  device: string,
): Promise<SentNote[]> {
  const sent: SentNote[] = [];
  for (const note of notes) {
    try {
      const accepted = await createCapture(note.text, device, {
        occurredAt: note.writtenAt,
        session,
      });
      // The backend has to say back the moment we asked it to keep. A
      // version that does not understand `occurred_at` would answer with
      // the time it received the note, and every note in a four-hour
      // session would quietly claim to have been thought at the moment
      // Send was pressed — wrong in a way nobody would notice until the
      // timeline was already full of it. Better a note that refuses to
      // send than one that lies about when you thought it.
      if (!sameMoment(accepted.occurred_at, note.writtenAt)) {
        sent.push({
          ...note,
          eventId: null,
          error:
            "the backend kept this at " +
            new Date(accepted.occurred_at).toLocaleTimeString() +
            " instead of when it was written — it is too old to accept a written time",
        });
        continue;
      }
      sent.push({ ...note, eventId: accepted.event_id, error: null });
    } catch (err) {
      sent.push({ ...note, eventId: null, error: String(err) });
    }
  }
  return sent;
}

/// A capture's judged echo. Comes back with `pending` set while the
/// judgement is still running — see `useEcho`, which follows it.
export function getEcho(captureEventId: string, minRerank?: number): Promise<EchoResponse> {
  const params = new URLSearchParams();
  if (minRerank !== undefined) params.set("min_rerank", String(minRerank));
  const query = params.toString();
  return request<EchoResponse>(`/captures/${captureEventId}/echo${query ? `?${query}` : ""}`);
}

/// What the backend has not finished yet. Mirrors
/// `contracts::PipelineStatus`.
export interface PipelineStatus {
  waiting: number;
  given_up: number;
}

/// Asked on a timer by the sidebar. Both numbers are normally zero; the
/// point is the moment they are not — a capture whose structuring failed
/// is otherwise completely invisible, because it is still in the timeline
/// and still findable by its words and simply never has any meaning
/// attached to it.
export function getPipelineStatus(): Promise<PipelineStatus> {
  return request<PipelineStatus>("/pipeline");
}

/// Folds one entity into another because you said so.
///
/// `sourceId` is the one that disappears from the graph; `intoId` is the
/// one that survives and inherits its observations, edges and names.
/// Nothing is deleted — the merge is an event and `unmergeEntity` takes
/// it back.
export function mergeEntities(sourceId: string, intoId: string): Promise<ChangeRecord[]> {
  return request<ChangeRecord[]>(`/entities/${sourceId}/merge`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ into: intoId }),
  });
}

export function unmergeEntity(sourceId: string): Promise<ChangeRecord[]> {
  return request<ChangeRecord[]>(`/entities/${sourceId}/unmerge`, { method: "POST" });
}

/// Takes a derived edge back — the "related" counterpart to `unmergeEntity`.
/// `relationId` is the change's `undo_id`, not an entity id.
export function retractRelation(relationId: string): Promise<ChangeRecord[]> {
  return request<ChangeRecord[]>(`/relations/${relationId}`, { method: "DELETE" });
}

/// How the view of your knowledge came to look the way it does. Your
/// notes never change; this is everything that happened to their
/// arrangement.
export function getChangelog(): Promise<ChangeRecord[]> {
  return request<ChangeRecord[]>("/consolidation");
}

/// One change the consolidation run made. Mirrors
/// `contracts::ChangeRecord`.
export interface ChangeRecord {
  kind: "merged" | "renamed" | "retyped" | "related";
  entity_id: string;
  entity_name: string;
  before: string;
  after: string;
  reason: string;
  changed_at: string;
  undo_id: string | null;
  undone: boolean;
}

/// What a consolidation pass would do, if it ran. Mirrors
/// `contracts::ConsolidationPreview`.
///
/// `token` names this exact preview for `applyConsolidation` — absent when
/// nothing was proposed. Applying replays this stored judgement minus
/// whatever item ids are excluded; it never asks the model again.
export interface ConsolidationPreview {
  same_name: PreviewMerge[];
  considered: number;
  merges: PreviewMerge[];
  retypes: PreviewRetype[];
  relations: PreviewRelation[];
  note: string | null;
  token: string | null;
}

export interface PreviewMerge {
  id: string;
  keep: string;
  absorb: string[];
  new_name: string | null;
  reason: string;
}

export interface PreviewRetype {
  id: string;
  entity: string;
  from: string;
  to: string;
  reason: string;
}

export interface PreviewRelation {
  id: string;
  from: string;
  to: string;
  relation_type: string;
  reason: string;
}

/// The same reasoning a real pass performs, applied and thrown away — and,
/// when it found anything, stored under the returned `token` so it can be
/// applied later.
export function getConsolidationPreview(): Promise<ConsolidationPreview> {
  return request<ConsolidationPreview>("/consolidation/preview");
}

export interface RunReport {
  merged_by_name: number;
  merged_by_judgement: number;
  retyped: number;
  related: number;
  examined: number;
  considered: number;
}

/// Carries out a stored preview, skipping whatever item ids are in
/// `exclude`. This is the normal way a pass runs: preview, remove what
/// looks wrong, apply the rest.
export function applyConsolidation(token: string, exclude: string[]): Promise<RunReport> {
  return request<RunReport>("/consolidation/apply", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, exclude }),
  });
}

/// Asks the backend for one more attempt at a capture it gave up on.
export function retryCapture(eventId: string): Promise<PipelineStatus> {
  return request<PipelineStatus>(`/captures/${eventId}/retry`, { method: "POST" });
}

/// Asks for one more attempt at everything the backend gave up on. The
/// sidebar's action, because the cause is nearly always shared.
export function retryStuck(): Promise<PipelineStatus> {
  return request<PipelineStatus>("/pipeline/retry", { method: "POST" });
}

export function getCapture(eventId: string): Promise<CaptureDetail> {
  return request<CaptureDetail>(`/captures/${eventId}`);
}

/// A source an <audio> element can play, for a capture's recording.
///
/// An `<audio src>` cannot carry the Access headers a request needs, so
/// behind Cloudflare Access the element would load a login page instead
/// of a recording. In the desktop app the bytes are therefore fetched in
/// Rust and handed over as a blob: playback works, at the cost of
/// streaming and seeking into a file that has not finished loading.
/// Captures are seconds long, so that cost is theoretical.
///
/// The returned URL must be handed to `releaseAudioSource` when the
/// player is done with it, or the blob stays in memory for the lifetime
/// of the window.
/// `mime` comes from the capture's own `audio.mime` rather than being
/// assumed: recordings from this app are WAV, but the backend accepts
/// Opus, Ogg and m4a too, and a blob typed wrongly simply refuses to play.
export async function audioSource(eventId: string, mime: string): Promise<string> {
  if (!runningInDesktopApp()) return `${baseUrl}/captures/${eventId}/audio`;

  const bytes = await apiAudio(eventId);
  return URL.createObjectURL(new Blob([bytes], { type: mime }));
}

/// Frees a source from `audioSource`. A plain URL is left alone.
export function releaseAudioSource(src: string): void {
  if (src.startsWith("blob:")) URL.revokeObjectURL(src);
}

/// What a redaction removed. Mirrors `contracts::Redacted`.
export interface Redacted {
  event_id: string;
  redacted_at: string;
  observations_removed: number;
  relations_removed: number;
  entities_removed: number;
  audio_removed: boolean;
}

/// Takes a capture's words back.
///
/// The event log keeps the fact that something was recorded and when;
/// everything else — the text, the recording, the entities and relations
/// derived from it, its place in the search index and in other captures'
/// echoes — is removed. The capture disappears from the timeline as a
/// result. This cannot be undone, and older backups still hold the
/// original.
export function redactCapture(eventId: string): Promise<Redacted> {
  return request<Redacted>(`/captures/${eventId}`, { method: "DELETE" });
}

/// Corrects a capture's text. The correction is appended as a new
/// transcript; nothing is overwritten, and the original stays readable.
export function correctTranscript(eventId: string, text: string): Promise<TranscriptVersion> {
  return request<TranscriptVersion>(`/captures/${eventId}/transcript`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ text }),
  });
}

export function listEntities(options: { entityType?: string; name?: string } = {}): Promise<
  EntityListItem[]
> {
  const params = new URLSearchParams();
  if (options.entityType) params.set("entity_type", options.entityType);
  if (options.name) params.set("name", options.name);
  const query = params.toString();
  return request<EntityListItem[]>(`/entities${query ? `?${query}` : ""}`);
}

/// What `id` could be folded into. Without `query`: the look-alikes by
/// name, then more of the same kind. With it: whatever that name finds,
/// aliases included.
export function getFoldCandidates(id: string, query?: string): Promise<EntityListItem[]> {
  const params = new URLSearchParams();
  if (query?.trim()) params.set("q", query.trim());
  const suffix = params.toString();
  return request<EntityListItem[]>(`/entities/${id}/fold-candidates${suffix ? `?${suffix}` : ""}`);
}

export function getEntity(id: string): Promise<EntityDetail> {
  return request<EntityDetail>(`/entities/${id}`);
}

/// What the system has to say without being asked.
export function getResurfaced(): Promise<Resurfaced> {
  return request<Resurfaced>("/resurface");
}

/// The whole graph at once — see `contracts::Graph` for why it is not
/// walked node by node.
export function getGraph(options: { limit?: number; entityType?: string } = {}): Promise<Graph> {
  const params = new URLSearchParams();
  if (options.limit) params.set("limit", String(options.limit));
  if (options.entityType) params.set("entity_type", options.entityType);
  const query = params.toString();
  return request<Graph>(`/graph${query ? `?${query}` : ""}`);
}

export function listEntityTypes(): Promise<EntityTypeCount[]> {
  return request<EntityTypeCount[]>("/entity-types");
}

/// The crate version the connected backend is actually running — useful
/// once two Macs share one NAS-hosted backend and "which build is live"
/// stops being obvious from being the one who deployed it.
export function getBackendVersion(): Promise<string> {
  return request<string>("/version");
}

export function search(query: string, options: SearchOptions = {}): Promise<SearchResult[]> {
  const params = new URLSearchParams({ query, limit: String(options.limit ?? 20) });
  if (options.entityType) params.set("entity_type", options.entityType);
  if (options.from) params.set("from", options.from);
  if (options.to) params.set("to", options.to);
  return request<SearchResult[]>(`/search?${params}`);
}
