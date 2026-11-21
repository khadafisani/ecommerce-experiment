import { expect, test } from 'bun:test';
import { MAX_UPLOAD_BYTES, prepareUpload, validateUploadSize } from '../src/upload-media';

test('sends original videos within the 10 MiB limit without re-encoding', async () => {
  const video = new File([new Uint8Array(MAX_UPLOAD_BYTES)], 'review.mp4', { type: 'video/mp4' });
  expect(await prepareUpload(video)).toBe(video);
});

test('rejects oversized videos before doing any encoding', () => {
  const video = new File([new Uint8Array(MAX_UPLOAD_BYTES + 1)], 'review.webm');
  expect(() => validateUploadSize(video)).toThrow('Videos must be 10 MiB or smaller');
});

test('encodes an oversized image within bounds and falls back when WebP is unavailable', async () => {
  const oldDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
  const oldBitmap = Object.getOwnPropertyDescriptor(globalThis, 'createImageBitmap');
  const image = new File([new Uint8Array(MAX_UPLOAD_BYTES + 1)], 'review.png', { type: 'image/png' });
  let closed = false;
  let drawn: number[] = [];
  const canvas = {
    width: 0,
    height: 0,
    getContext: () => ({ drawImage: (_: unknown, ...bounds: number[]) => drawn = bounds }),
    toBlob: (callback: (blob: Blob) => void, mime: string) =>
      callback(new Blob(['encoded'], { type: mime === 'image/webp' ? 'image/png' : mime })),
  };
  Object.defineProperties(globalThis, {
    document: { value: { createElement: () => canvas }, configurable: true },
    createImageBitmap: { value: async () => ({ width: 3000, height: 1500, close: () => closed = true }), configurable: true },
  });
  try {
    const prepared = await prepareUpload(image);
    expect(drawn).toEqual([0, 0, 1440, 720]);
    expect(closed).toBe(true);
    expect(canvas.width).toBe(0);
    expect(canvas.height).toBe(0);
    expect(prepared.type).toBe('image/jpeg');
    expect(prepared.name).toBe('review.jpg');
    expect(prepared.size).toBeLessThanOrEqual(MAX_UPLOAD_BYTES);
  } finally {
    for (const [name, descriptor] of [['document', oldDocument], ['createImageBitmap', oldBitmap]] as const) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else Reflect.deleteProperty(globalThis, name);
    }
  }
});
