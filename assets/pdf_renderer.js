/**
 * PDF rendering helper for Cantara presentation view.
 * Uses PDF.js (loaded as an ES module) to render individual PDF pages to canvas elements.
 *
 * Usage from Rust/Dioxus:
 *   1. Call initPdfJs(pdfjsUrl, workerUrl) once to load the library.
 *   2. Call renderPdfPage(base64Data, cacheKey, pageNum, canvasId) for each slide.
 */

// Global state
window.__pdfjsLib = null;
window.__pdfjsInitPromise = null;
window.__pdfDocCache = {};

/**
 * Initialise PDF.js by dynamically importing the ES module.
 * Safe to call multiple times – only the first call triggers the import.
 *
 * @param {string} pdfjsUrl   – served URL of pdf.min.mjs (from asset!())
 * @param {string} workerUrl  – served URL of pdf.worker.min.mjs
 */
window.initPdfJs = function (pdfjsUrl, workerUrl) {
  if (!window.__pdfjsInitPromise) {
    window.__pdfjsInitPromise = import(pdfjsUrl).then(function (lib) {
      lib.GlobalWorkerOptions.workerSrc = workerUrl;
      window.__pdfjsLib = lib;
    });
  }
  return window.__pdfjsInitPromise;
};

/**
 * Render a single page of a PDF onto a <canvas>.
 *
 * @param {string} base64Data  – the entire PDF file encoded as base64
 * @param {string} cacheKey    – a stable key for caching the parsed document
 * @param {number} pageNum     – 1-based page number
 * @param {string} canvasId    – DOM id of the target <canvas>
 */
window.renderPdfPage = async function (base64Data, cacheKey, pageNum, canvasId) {
  try {
    // Wait until PDF.js is ready
    if (!window.__pdfjsLib) {
      if (window.__pdfjsInitPromise) {
        await window.__pdfjsInitPromise;
      } else {
        console.error("PDF.js not initialised – call initPdfJs first");
        return;
      }
    }

    var pdfjsLib = window.__pdfjsLib;

    // Decode & cache the PDF document
    if (!window.__pdfDocCache[cacheKey]) {
      var binaryString = atob(base64Data);
      var bytes = new Uint8Array(binaryString.length);
      for (var i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }
      window.__pdfDocCache[cacheKey] = await pdfjsLib.getDocument({ data: bytes }).promise;
    }

    var pdf = window.__pdfDocCache[cacheKey];
    var page = await pdf.getPage(pageNum);

    var canvas = document.getElementById(canvasId);
    if (!canvas) return;

    var ctx = canvas.getContext("2d");
    var container = canvas.parentElement;
    var containerWidth = container.clientWidth || 800;
    var containerHeight = container.clientHeight || 600;

    var vp = page.getViewport({ scale: 1 });
    var scale = Math.min(containerWidth / vp.width, containerHeight / vp.height);
    var viewport = page.getViewport({ scale: scale });

    canvas.width = viewport.width;
    canvas.height = viewport.height;

    await page.render({ canvasContext: ctx, viewport: viewport }).promise;
  } catch (e) {
    console.error("PDF render error:", e);
  }
};
