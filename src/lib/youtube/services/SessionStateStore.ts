import type { DeveloperSnapshot, PlaybackState, SchedulerStats, SessionState } from '../types';

export class SessionStateStore extends EventTarget {
  private state: SessionState = {
    playbackState: 'idle',
    lastError: null,
    currentTime: 0,
    duration: 0,
    lastRemark: '',
    lastDecisionReason: 'idle',
    nextCommentProbability: 0,
    currentSegmentText: '',
    transcriptMode: 'metadata',
    transcriptStatusMessage: 'No transcript loaded yet.',
    transcriptQuality: 'low',
    transcriptCoverage: 0
  };

  private stats: SchedulerStats = {
    remarksSpokenThisMinute: 0,
    secondsSinceLastRemark: 999,
    repetitionMemory: [],
    skippedOpportunities: 0
  };

  private debug: DeveloperSnapshot = {
    timestamp: 0,
    transcriptWindow: null,
    commentDecision: null,
    fired: false,
    reason: 'idle'
  };

  getSnapshot(): SessionState {
    return { ...this.state };
  }

  getStats(): SchedulerStats {
    return {
      ...this.stats,
      repetitionMemory: [...this.stats.repetitionMemory]
    };
  }

  getDebug(): DeveloperSnapshot {
    return {
      ...this.debug,
      transcriptWindow: this.debug.transcriptWindow,
      commentDecision: this.debug.commentDecision
    };
  }

  setPlaybackState(next: PlaybackState): void {
    this.state.playbackState = next;
    this.emit();
  }

  setError(message: string | null): void {
    this.state.lastError = message;
    if (message) this.state.playbackState = 'error';
    this.emit();
  }

  setTimeline(currentTime: number, duration: number): void {
    this.state.currentTime = currentTime;
    this.state.duration = duration;
    this.emit();
  }

  setCurrentSegment(text: string): void {
    this.state.currentSegmentText = text;
    this.emit();
  }

  setTranscriptMode(mode: SessionState['transcriptMode']): void {
    this.state.transcriptMode = mode;
    this.emit();
  }

  setTranscriptStatus(message: string, quality: SessionState['transcriptQuality'], coverage: number): void {
    this.state.transcriptStatusMessage = message;
    this.state.transcriptQuality = quality;
    this.state.transcriptCoverage = Math.max(0, Math.min(1, coverage));
    this.emit();
  }

  setLastDecision(reason: string, probability: number): void {
    this.state.lastDecisionReason = reason;
    this.state.nextCommentProbability = Math.max(0, Math.min(1, probability));
    this.emit();
  }

  markRemarkSpoken(remark: string): void {
    this.state.lastRemark = remark;
    this.stats.remarksSpokenThisMinute += 1;
    this.stats.secondsSinceLastRemark = 0;
    this.stats.repetitionMemory = [remark, ...this.stats.repetitionMemory].slice(0, 16);
    this.emit();
  }

  tickSecond(): void {
    this.stats.secondsSinceLastRemark += 1;
    this.emit();
  }

  markSkipped(): void {
    this.stats.skippedOpportunities += 1;
    this.emit();
  }

  setDebug(snapshot: DeveloperSnapshot): void {
    this.debug = snapshot;
    this.emit();
  }

  clearOnSeek(): void {
    this.state.lastDecisionReason = 'seek detected - context reset';
    this.state.nextCommentProbability = 0;
    this.state.currentSegmentText = '';
    this.emit();
  }

  private emit(): void {
    this.dispatchEvent(new CustomEvent('change', { detail: this.getSnapshot() }));
  }
}
