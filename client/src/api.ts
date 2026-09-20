// Mirrors the DTOs in the `contracts` Rust crate. Kept as plain types (not
// generated) since the surface is small; if it grows, revisit codegen.

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
}

export interface CaptureAccepted {
  event_id: string;
  occurred_at: string;
  echo: EchoItem[];
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

export interface SearchResult {
  capture_event_id: string;
  transcript_text: string;
  occurred_at: string;
  score: number;
  related_entities: EntitySummary[];
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

const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "http://localhost:8080";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, init);
  if (!res.ok) {
    throw new Error(`${path} failed: ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as T;
}

export function listCaptures(): Promise<CaptureListItem[]> {
  return request<CaptureListItem[]>("/captures");
}

/// Records a capture. The response carries its echoes inline, so the user
/// sees them without a second round trip.
export function createCapture(transcriptText: string, device: string): Promise<CaptureAccepted> {
  return request<CaptureAccepted>("/captures", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ transcript_text: transcriptText, device }),
  });
}

export function getEcho(captureEventId: string, minSimilarity?: number): Promise<EchoItem[]> {
  const params = new URLSearchParams();
  if (minSimilarity !== undefined) params.set("min_similarity", String(minSimilarity));
  const query = params.toString();
  return request<EchoItem[]>(`/captures/${captureEventId}/echo${query ? `?${query}` : ""}`);
}

export function getCapture(eventId: string): Promise<CaptureDetail> {
  return request<CaptureDetail>(`/captures/${eventId}`);
}

/// URL of the original recording. Used as an <audio> source rather than
/// fetched, so the browser can stream and seek it itself.
export function audioUrl(eventId: string): string {
  return `${BASE_URL}/captures/${eventId}/audio`;
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

export function listEntityTypes(): Promise<EntityTypeCount[]> {
  return request<EntityTypeCount[]>("/entity-types");
}

export function search(query: string, options: SearchOptions = {}): Promise<SearchResult[]> {
  const params = new URLSearchParams({ query, limit: String(options.limit ?? 20) });
  if (options.entityType) params.set("entity_type", options.entityType);
  if (options.from) params.set("from", options.from);
  if (options.to) params.set("to", options.to);
  return request<SearchResult[]>(`/search?${params}`);
}
