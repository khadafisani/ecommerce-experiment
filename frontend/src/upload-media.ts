export const MAX_UPLOAD_BYTES = 10 * 1024 * 1024;
const MAX_IMAGE_EDGE = 1440;

function isImage(file: File) {
  return file.type.startsWith('image/') ||
    (!file.type && /\.(avif|gif|heic|heif|jpe?g|png|webp|bmp|tiff?)$/i.test(file.name));
}

export function validateUploadSize(file: File) {
  if (file.size <= MAX_UPLOAD_BYTES || isImage(file)) return;
  if (file.type.startsWith('video/') || /\.(mp4|webm)$/i.test(file.name)) {
    throw new Error('Videos must be 10 MiB or smaller. Trim or compress the video before uploading.');
  }
  throw new Error('This file exceeds the 10 MiB upload limit.');
}

export async function prepareUpload(file: File, signal?: AbortSignal) {
  signal?.throwIfAborted();
  validateUploadSize(file);
  if (file.size <= MAX_UPLOAD_BYTES) return file;

  const canvas = document.createElement('canvas');
  try {
    const image = await createImageBitmap(file);
    try {
      signal?.throwIfAborted();
      const scale = Math.min(1, MAX_IMAGE_EDGE / Math.max(image.width, image.height));
      canvas.width = Math.max(1, Math.round(image.width * scale));
      canvas.height = Math.max(1, Math.round(image.height * scale));
      const context = canvas.getContext('2d');
      if (!context) throw new Error('Image canvas unavailable');
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
    } finally {
      image.close();
    }

    for (const [mime, extension, quality] of [
      ['image/webp', 'webp', 0.85],
      ['image/jpeg', 'jpg', 0.82],
    ] as const) {
      const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, mime, quality));
      signal?.throwIfAborted();
      if (blob?.type === mime && blob.size <= MAX_UPLOAD_BYTES) {
        const name = file.name.replace(/\.[^.]+$/, '');
        return new File([blob], name + '.' + extension, { type: mime, lastModified: file.lastModified });
      }
    }
  } catch {
    signal?.throwIfAborted();
  } finally {
    canvas.width = canvas.height = 0;
  }
  throw new Error('This image could not be reduced for upload. Save it as a JPEG or WebP under 10 MiB and try again.');
}
