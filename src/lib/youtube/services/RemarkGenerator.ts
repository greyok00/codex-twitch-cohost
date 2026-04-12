import { z } from 'zod';
import { generateYoutubeRemark } from '../../api/tauri';
import type { RemarkGenerationRequest, RemarkResponse } from '../types';

const remarkSchema = z.object({
  shouldSpeak: z.boolean(),
  remark: z.string().default(''),
  anchor: z.string().default(''),
  topic: z.string().default(''),
  confidence: z.number().min(0).max(1).default(0),
  style: z.enum(['dry', 'sarcastic', 'chaotic', 'deadpan', 'absurd']),
  estimatedDurationSeconds: z.number().min(1).max(20).default(4),
  skipReason: z.string().nullable().default(null)
});

function scrubSpokenText(input: string): string {
  return (input || '')
    .replace(/\*[^*]{1,120}\*/g, ' ')
    .replace(/\([^)]{1,120}\)/g, ' ')
    .replace(/_[^_]{1,120}_/g, ' ')
    .replace(/[\p{Extended_Pictographic}\uFE0F]/gu, ' ')
    .replace(/[<>{}[\]\\/|]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export class RemarkGenerator {
  async generate(input: RemarkGenerationRequest): Promise<RemarkResponse> {
    const response = await generateYoutubeRemark(input);
    const parsed = remarkSchema.safeParse(response);
    if (!parsed.success) {
      return {
        shouldSpeak: false,
        remark: '',
        anchor: '',
        topic: input.context.topicSummary,
        confidence: 0,
        style: input.humorStyle,
        estimatedDurationSeconds: 3,
        skipReason: 'model response schema mismatch'
      };
    }
    const value = parsed.data;
    const remark = scrubSpokenText(value.remark);
    if (value.shouldSpeak && remark.length < 4) {
      return {
        ...value,
        shouldSpeak: false,
        remark: '',
        skipReason: 'remark too short after cleanup'
      };
    }
    return { ...value, remark };
  }
}
