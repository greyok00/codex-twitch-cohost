export type HumorStyle = 'dry' | 'sarcastic' | 'chaotic' | 'deadpan' | 'absurd';
export type PlaybackState = 'idle' | 'loading_video' | 'ready' | 'playing' | 'evaluating_comment' | 'paused_for_remark' | 'speaking_remark' | 'resuming' | 'ended' | 'error';

export interface TranscriptSegment {
  startTime: number;
  endTime: number;
  text: string;
  confidence?: number;
  speaker?: string;
}

export interface TopicContextWindow {
  currentTime: number;
  currentSegment: TranscriptSegment | null;
  previousSegments: TranscriptSegment[];
  nextSegments: TranscriptSegment[];
  topicSummary: string;
  entities: string[];
  tone: 'neutral' | 'serious' | 'excited' | 'tense' | 'playful';
  pauseConfidence: number;
  seriousnessScore: number;
  humorOpportunityScore: number;
  transcriptCoverageScore: number;
}

export interface YoutubeCohostSettings {
  remarksPerMinute: number;
  relevanceStrictness: number;
  humorStyle: HumorStyle;
  maxRemarkLengthSeconds: 4 | 8 | 12;
  interruptOnlyAtNaturalBreaks: boolean;
  captionsDebugOverlay: boolean;
  autoResumeAfterRemark: boolean;
  developerMode: boolean;
}

export interface SchedulerStats {
  remarksSpokenThisMinute: number;
  secondsSinceLastRemark: number;
  repetitionMemory: string[];
  skippedOpportunities: number;
}

export interface CommentScoreComponents {
  pauseConfidence: number;
  humorOpportunity: number;
  noveltyScore: number;
  transcriptCoverage: number;
  userSliderPressure: number;
  repetitionPenalty: number;
  seriousnessPenalty: number;
  total: number;
}

export interface CommentDecision {
  shouldInterrupt: boolean;
  reason: string;
  minGapSeconds: number;
  threshold: number;
  components: CommentScoreComponents;
}

export interface RemarkResponse {
  shouldSpeak: boolean;
  remark: string;
  anchor: string;
  topic: string;
  confidence: number;
  style: HumorStyle;
  estimatedDurationSeconds: number;
  skipReason: string | null;
}

export interface RemarkGenerationRequest {
  context: TopicContextWindow;
  humorStyle: HumorStyle;
  maxRemarkLengthSeconds: 4 | 8 | 12;
  relevanceStrictness: number;
  modelMode?: 'fast' | 'medium' | 'long_context';
  repetitionMemory: string[];
  topicHistory?: string[];
  recentRemarks?: string[];
  personalityPrompt?: string;
}

export interface SessionState {
  playbackState: PlaybackState;
  lastError: string | null;
  currentTime: number;
  duration: number;
  lastRemark: string;
  lastDecisionReason: string;
  nextCommentProbability: number;
  currentSegmentText: string;
  transcriptMode: 'provider' | 'user_file' | 'metadata';
  transcriptStatusMessage: string;
  transcriptQuality: 'high' | 'medium' | 'low';
  transcriptCoverage: number;
}

export interface DeveloperSnapshot {
  timestamp: number;
  transcriptWindow: TopicContextWindow | null;
  commentDecision: CommentDecision | null;
  fired: boolean;
  reason: string;
}

export interface PlayerTickEvent {
  currentTime: number;
  duration: number;
}

export interface PlayerSeekEvent {
  from: number;
  to: number;
}

export interface TranscriptSourceResult {
  mode: 'provider' | 'user_file' | 'metadata';
  segments: TranscriptSegment[];
  quality: 'high' | 'medium' | 'low';
  coverageScore: number;
  providerName: string;
  message: string;
}

export interface PlaylistInfo {
  videoId: string;
  playlistId?: string;
  startSeconds?: number;
}

export interface TranscriptLoadInput {
  videoId: string;
  title?: string;
  description?: string;
  durationSeconds?: number;
  transcriptFileText?: string;
}

export interface YoutubeHistoryItem {
  videoId: string;
  title: string;
  watchedAt: string;
  channelTitle?: string;
}
