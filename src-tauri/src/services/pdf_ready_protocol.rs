pub(crate) const PAGE_LOADED: &str = "pageLoaded";
pub(crate) const DOM_READY: &str = "domReady";
pub(crate) const FONTS_SETTLED: &str = "fontsSettled";
pub(crate) const IMAGES_SETTLED: &str = "imagesSettled";
pub(crate) const LAYOUT_READY: &str = "layoutReady";

pub(crate) const DOM_TIMEOUT: &str = "PDF_DOM_TIMEOUT";
pub(crate) const DOM_FAILED: &str = "PDF_DOM_FAILED";
pub(crate) const FONT_TIMEOUT: &str = "PDF_FONT_TIMEOUT";
pub(crate) const FONT_FAILED: &str = "PDF_FONT_FAILED";
pub(crate) const IMAGE_TIMEOUT: &str = "PDF_IMAGE_TIMEOUT";
pub(crate) const IMAGE_DECODE_FAILED: &str = "PDF_IMAGE_DECODE_FAILED";
pub(crate) const LAYOUT_FAILED: &str = "PDF_LAYOUT_FAILED";

pub(crate) const RENDER_PHASES: [&str; 5] = [
    PAGE_LOADED,
    DOM_READY,
    FONTS_SETTLED,
    IMAGES_SETTLED,
    LAYOUT_READY,
];

pub(crate) const FAILURE_SIGNALS: [(&str, &str); 7] = [
    (PAGE_LOADED, DOM_TIMEOUT),
    (PAGE_LOADED, DOM_FAILED),
    (FONTS_SETTLED, FONT_TIMEOUT),
    (FONTS_SETTLED, FONT_FAILED),
    (IMAGES_SETTLED, IMAGE_TIMEOUT),
    (IMAGES_SETTLED, IMAGE_DECODE_FAILED),
    (LAYOUT_READY, LAYOUT_FAILED),
];

const PDF_READY_SCRIPT_TEMPLATE: &str = r#"<script>
(() => {
  'use strict';
  const token = __MARKLITE_PDF_READY_TOKEN__;
  const protocol = Object.freeze({
    stages: Object.freeze({
      pageLoaded: __MARKLITE_STAGE_PAGE_LOADED__,
      domReady: __MARKLITE_STAGE_DOM_READY__,
      fontsSettled: __MARKLITE_STAGE_FONTS_SETTLED__,
      imagesSettled: __MARKLITE_STAGE_IMAGES_SETTLED__,
      layoutReady: __MARKLITE_STAGE_LAYOUT_READY__
    }),
    codes: Object.freeze({
      domTimeout: __MARKLITE_CODE_DOM_TIMEOUT__,
      domFailed: __MARKLITE_CODE_DOM_FAILED__,
      fontTimeout: __MARKLITE_CODE_FONT_TIMEOUT__,
      fontFailed: __MARKLITE_CODE_FONT_FAILED__,
      imageTimeout: __MARKLITE_CODE_IMAGE_TIMEOUT__,
      imageDecodeFailed: __MARKLITE_CODE_IMAGE_DECODE_FAILED__,
      layoutFailed: __MARKLITE_CODE_LAYOUT_FAILED__
    })
  });
  const stageTimeoutMs = 8000;
  const completed = [];
  let terminalSent = false;

  const send = (kind, stage, code = '', imageCount = 0, imageFailed = 0) => {
    if (terminalSent) return;
    const endpoint = kind === 'ready'
      ? 'marklite-export://ready'
      : 'marklite-export://error';
    const params = new URLSearchParams({
      token,
      stage,
      code,
      completed: completed.join(','),
      images: String(imageCount),
      failed: String(imageFailed)
    });
    window.location.href = `${endpoint}?${params.toString()}`;
    terminalSent = true;
  };

  const stageError = (code) => Object.assign(new Error(code), { code });
  const errorCode = (error, fallback) => error && typeof error.code === 'string'
    ? error.code
    : fallback;
  const bounded = (promise, timeoutCode, failureCode) => new Promise((resolve, reject) => {
    let settled = false;
    const timer = window.setTimeout(() => {
      if (settled) return;
      settled = true;
      reject(stageError(timeoutCode));
    }, stageTimeoutMs);
    Promise.resolve(promise).then(
      (value) => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        resolve(value);
      },
      () => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        reject(stageError(failureCode));
      }
    );
  });

  const waitForDom = () => document.readyState === 'loading'
    ? new Promise((resolve) => document.addEventListener('DOMContentLoaded', resolve, { once: true }))
    : Promise.resolve();

  const waitForImage = (image) => {
    if (typeof image.decode === 'function') return image.decode();
    if (image.complete) {
      return image.naturalWidth > 0
        ? Promise.resolve()
        : Promise.reject(stageError(protocol.codes.imageDecodeFailed));
    }
    return new Promise((resolve, reject) => {
      image.addEventListener('load', resolve, { once: true });
      image.addEventListener('error', () => reject(stageError(protocol.codes.imageDecodeFailed)), { once: true });
    });
  };

  (async () => {
    try {
      await bounded(waitForDom(), protocol.codes.domTimeout, protocol.codes.domFailed);
    } catch (error) {
      send('error', protocol.stages.pageLoaded, errorCode(error, protocol.codes.domFailed));
      return;
    }
    completed.push(protocol.stages.pageLoaded, protocol.stages.domReady);

    try {
      await bounded(
        document.fonts ? document.fonts.ready : Promise.resolve(),
        protocol.codes.fontTimeout,
        protocol.codes.fontFailed
      );
    } catch (error) {
      send('error', protocol.stages.fontsSettled, errorCode(error, protocol.codes.fontFailed));
      return;
    }
    completed.push(protocol.stages.fontsSettled);

    const images = Array.from(document.images);
    const imageResults = await Promise.all(images.map(async (image) => {
      try {
        await bounded(waitForImage(image), protocol.codes.imageTimeout, protocol.codes.imageDecodeFailed);
        return '';
      } catch (error) {
        return errorCode(error, protocol.codes.imageDecodeFailed);
      }
    }));
    const imageFailures = imageResults.filter(Boolean);
    if (imageFailures.length > 0) {
      const code = imageFailures.includes(protocol.codes.imageTimeout)
        ? protocol.codes.imageTimeout
        : protocol.codes.imageDecodeFailed;
      send('error', protocol.stages.imagesSettled, code, images.length, imageFailures.length);
      return;
    }
    completed.push(protocol.stages.imagesSettled);

    try {
      document.documentElement.getBoundingClientRect();
      if (document.body) document.body.getBoundingClientRect();
      window.getComputedStyle(document.documentElement).getPropertyValue('width');
      await Promise.resolve();
      void document.documentElement.scrollHeight;
    } catch (_) {
      send('error', protocol.stages.layoutReady, protocol.codes.layoutFailed, images.length, 0);
      return;
    }
    send('ready', protocol.stages.layoutReady, '', images.length, 0);
  })().catch(() => send('error', protocol.stages.layoutReady, protocol.codes.layoutFailed));
})();
</script>"#;

pub(crate) fn render_ready_script(token: &str) -> String {
    let values = [
        ("__MARKLITE_PDF_READY_TOKEN__", token),
        ("__MARKLITE_STAGE_PAGE_LOADED__", PAGE_LOADED),
        ("__MARKLITE_STAGE_DOM_READY__", DOM_READY),
        ("__MARKLITE_STAGE_FONTS_SETTLED__", FONTS_SETTLED),
        ("__MARKLITE_STAGE_IMAGES_SETTLED__", IMAGES_SETTLED),
        ("__MARKLITE_STAGE_LAYOUT_READY__", LAYOUT_READY),
        ("__MARKLITE_CODE_DOM_TIMEOUT__", DOM_TIMEOUT),
        ("__MARKLITE_CODE_DOM_FAILED__", DOM_FAILED),
        ("__MARKLITE_CODE_FONT_TIMEOUT__", FONT_TIMEOUT),
        ("__MARKLITE_CODE_FONT_FAILED__", FONT_FAILED),
        ("__MARKLITE_CODE_IMAGE_TIMEOUT__", IMAGE_TIMEOUT),
        ("__MARKLITE_CODE_IMAGE_DECODE_FAILED__", IMAGE_DECODE_FAILED),
        ("__MARKLITE_CODE_LAYOUT_FAILED__", LAYOUT_FAILED),
    ];
    let mut rendered = PDF_READY_SCRIPT_TEMPLATE.to_string();
    for (placeholder, value) in values {
        let json = serde_json::to_string(value)
            .expect("serializing an internal PDF ready protocol value cannot fail");
        rendered = rendered.replace(placeholder, &json);
    }
    debug_assert!(!rendered.contains("__MARKLITE_"));
    rendered
}

#[cfg(test)]
mod tests {
    use super::{render_ready_script, FAILURE_SIGNALS, RENDER_PHASES};

    #[test]
    fn rendered_script_contains_only_the_shared_wire_contract() {
        let script = render_ready_script("ready-token");
        assert!(!script.contains("__MARKLITE_"));
        assert!(script.contains("\"ready-token\""));
        for phase in RENDER_PHASES {
            assert!(script.contains(&serde_json::to_string(phase).unwrap()));
        }
        for (_, code) in FAILURE_SIGNALS {
            assert!(script.contains(&serde_json::to_string(code).unwrap()));
        }
        assert!(!script.contains("timeoutCode.replace"));
    }
}
