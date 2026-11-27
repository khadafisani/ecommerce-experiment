"""Local GLiNER inference endpoint for the catalog worker."""

import argparse
import json
from functools import partial
from http.server import BaseHTTPRequestHandler, HTTPServer

FASTINO_MODEL = "fastino/gliner2.5-multi-v1"
SMALL_MODEL = "gliner-community/gliner_small-v2.5"
INDONESIAN_MODEL = "muchad/gliner-id"
MULTILINGUAL_MODEL = "urchade/gliner_multi-v2.1"
GLINER_CHECKPOINTS = {
    SMALL_MODEL: ("f227d3cd637bd4e6757ae143935316d062393341", "pytorch_model.bin"),
    INDONESIAN_MODEL: ("89dd811973d6a72e6e5f01b932d5379121829f9b", "pytorch_model.bin"),
    MULTILINGUAL_MODEL: ("443d26d654e0324125a96bebd8e796c14ff2efe6", "model.safetensors"),
}
PRODUCT_LABELS = {
    "brand": "Brand or manufacturer of the item being sold",
    "product": "Specific name or model of the item being sold, including the model number and generation",
    "product_type": "Type of item being sold, such as running shorts, smartphone, or phone case",
    "compatibility": "Name or model of a different item that the sold item fits or is compatible with",
}
FASTINO_SCHEMAS = {
    "product": PRODUCT_LABELS,
    "variant": {"option": "Variant value, such as color, size, storage capacity, or condition"},
    "query": {
        "brand": "Brand the shopper is looking for",
        "product": "Name or model the shopper is looking for, even when misspelled",
        "product_type": "Type of product the shopper is looking for",
    },
}
GLINER_PRODUCT_LABELS = {"brand": "brand", "product": "product", "product category": "product_type"}
GLINER_LABELS = {
    "product": GLINER_PRODUCT_LABELS,
    "variant": dict.fromkeys(("color", "size", "storage capacity", "condition"), "option"),
    "query": GLINER_PRODUCT_LABELS,
}


def best_span(text, mentions):
    """Return a source span. GLiNER's end offset has varied across loaders."""
    for mention in sorted(mentions, key=lambda item: item["confidence"], reverse=True):
        value = mention["text"].strip()
        if value and value in text:
            return value
    return None


def extract_fastino(model, batch_size, task, texts):
    schema = FASTINO_SCHEMAS[task]
    outputs = model.batch_extract_entities(
        texts, schema, batch_size=batch_size, threshold=0.5,
        include_spans=True, include_confidence=True,
    )
    if len(outputs) != len(texts):
        raise RuntimeError("GLiNER returned an unexpected number of results")
    return [
        {
            label: best_span(text, mentions)
            for label, mentions in result["entities"].items()
            if label in schema and mentions
        }
        for text, result in zip(texts, outputs)
    ]


def extract_gliner(model, batch_size, task, texts):
    labels = GLINER_LABELS[task]
    outputs = model.inference(texts, list(labels), batch_size=batch_size, threshold=0.5)
    if len(outputs) != len(texts):
        raise RuntimeError("GLiNER returned an unexpected number of results")
    results = []
    for text, mentions in zip(texts, outputs):
        result = {}
        for mention in sorted(mentions, key=lambda item: item["score"], reverse=True):
            field = labels.get(mention["label"])
            value = mention["text"].strip()
            if field and value and value in text:
                result.setdefault(field, value)
        results.append(result)
    return results


def serve(extract, port, model_name, device):
    class Handler(BaseHTTPRequestHandler):
        def send_json(self, status, body):
            data = json.dumps(body, ensure_ascii=False).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            if self.path == "/health":
                self.send_json(200, {"status": "ready", "model": model_name, "device": device})
            else:
                self.send_json(404, {"error": "unknown path"})

        def do_POST(self):
            if self.path != "/extract":
                self.send_json(404, {"error": "unknown path"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 1 <= length <= 128_000:
                    raise ValueError("invalid request size")
                request = json.loads(self.rfile.read(length))
                task = request.get("task") if isinstance(request, dict) else None
                texts = request.get("texts") if isinstance(request, dict) else None
                if (
                    not isinstance(task, str) or task not in FASTINO_SCHEMAS
                    or not isinstance(texts, list) or not 1 <= len(texts) <= 64
                    or not all(isinstance(text, str) and 1 <= len(text) <= 512 for text in texts)
                ):
                    raise ValueError("invalid task or texts")
            except (ValueError, UnicodeError):
                self.send_json(400, {"error": "invalid extraction request"})
                return
            try:
                results = extract(task, texts)
            except Exception as error:
                print(f"GLiNER extraction failed: {type(error).__name__}", flush=True)
                self.send_json(500, {"error": "GLiNER extraction failed"})
                return
            self.send_json(200, {"results": results})

        def log_message(self, *_):
            pass

    with HTTPServer(("127.0.0.1", port), Handler) as server:
        print(f"GLiNER ready at http://127.0.0.1:{port}", flush=True)
        server.serve_forever()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=11435)
    parser.add_argument("--batch-size", type=int, choices=range(1, 65), default=16)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--device", choices=("cpu", "xpu", "cuda"), default="cpu")
    parser.add_argument("--model", choices=(FASTINO_MODEL, *GLINER_CHECKPOINTS), default=FASTINO_MODEL)
    options = parser.parse_args()

    import torch

    torch.set_num_threads(options.threads)
    print(f"Loading {options.model} on {options.device}", flush=True)
    if options.model in GLINER_CHECKPOINTS:
        from gliner import GLiNER
        from huggingface_hub import snapshot_download

        revision, weights = GLINER_CHECKPOINTS[options.model]
        snapshot = snapshot_download(
            options.model, revision=revision, allow_patterns=["*.json", "spm.model", weights],
        )
        model = GLiNER.from_pretrained(snapshot, map_location="cpu")
        extract = extract_gliner
    else:
        from gliner2 import AutoExtractor

        model = AutoExtractor.from_pretrained(options.model, map_location="cpu")
        extract = extract_fastino
    model = model.to(options.device).eval()
    device = str(next(model.parameters()).device)
    serve(partial(extract, model, options.batch_size), options.port, options.model, device)


if __name__ == "__main__":
    main()
