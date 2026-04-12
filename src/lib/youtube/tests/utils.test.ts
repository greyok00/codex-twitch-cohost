import { describe, expect, it } from 'vitest';
import { parseYouTubeHistory, parseYouTubeInput } from '../utils';

describe('youtube utils', () => {
  it('parses playlist URLs with start seconds', () => {
    const parsed = parseYouTubeInput('https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL1234567890&t=43s');
    expect(parsed?.videoId).toBe('dQw4w9WgXcQ');
    expect(parsed?.playlistId).toBe('PL1234567890');
    expect(parsed?.startSeconds).toBe(43);
  });

  it('parses youtube history exports', () => {
    const history = parseYouTubeHistory(
      JSON.stringify([
        {
          title: 'Watched cool thing',
          titleUrl: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
          time: '2026-04-12T00:00:00.000Z',
          subtitles: [{ name: 'Example Channel' }]
        }
      ])
    );
    expect(history).toHaveLength(1);
    expect(history[0].videoId).toBe('dQw4w9WgXcQ');
  });
});
