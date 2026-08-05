import Foundation

/// JavaScript injected into the reader.
///
/// Runs in a private content world, so the document itself cannot see or call
/// any of it — and combined with the sanitizer and the page CSP, a document
/// cannot run script of its own at all.
enum ReaderBridgeScript {
    static let source = """
    (function () {
      const bridge = {};
      const post = (name, payload) => {
        try { window.webkit.messageHandlers[name].postMessage(payload); } catch (_) {}
      };

      // ---- Scroll spy ------------------------------------------------------
      // Reports the heading whose top has most recently passed the viewport top,
      // which is what the sidebar outline highlights.
      let lastHeading = null;
      const headings = () => Array.from(document.querySelectorAll('h1,h2,h3,h4,h5,h6'));

      function reportActiveHeading() {
        const marker = 48;
        let active = null;
        for (const heading of headings()) {
          if (heading.getBoundingClientRect().top <= marker) { active = heading; } else { break; }
        }
        if (!active) { active = headings()[0] || null; }
        const id = active ? active.id : null;
        if (id !== lastHeading) { lastHeading = id; post('activeHeading', id); }
      }

      let lastOffset = -1;
      function reportScrollOffset() {
        const offset = Math.max(0, Math.round(window.scrollY));
        if (offset === lastOffset) return;
        lastOffset = offset;
        post('scrollOffset', offset);
      }

      let scrollScheduled = false;
      window.addEventListener('scroll', () => {
        if (scrollScheduled) return;
        scrollScheduled = true;
        requestAnimationFrame(() => {
          scrollScheduled = false;
          reportActiveHeading();
          reportScrollOffset();
        });
      }, { passive: true });

      // ---- Links -----------------------------------------------------------
      // Anchors are handled in-page; everything else goes to the app to decide.
      document.addEventListener('click', (event) => {
        const anchor = event.target.closest ? event.target.closest('a[href]') : null;
        if (!anchor) return;
        event.preventDefault();
        const href = anchor.getAttribute('href') || '';
        if (href.startsWith('#')) {
          bridge.scrollToAnchor(href.slice(1));
        } else {
          post('openLink', href);
        }
      });

      bridge.scrollToAnchor = function (slug) {
        const target = document.querySelector('[data-anchor="' + CSS.escape(slug) + '"]')
          || document.getElementById(slug);
        if (target) {
          target.scrollIntoView({ behavior: 'smooth', block: 'start' });
          post('activeHeading', target.id);
        }
      };

      bridge.scrollToBlock = function (id) {
        const target = document.getElementById(id);
        if (target) { target.scrollIntoView({ behavior: 'smooth', block: 'start' }); }
      };

      // ---- Selection -------------------------------------------------------
      // A selection is reported as an offset range inside its nearest block, so
      // an annotation survives re-rendering as long as that block is unchanged.
      function blockFor(node) {
        let element = node.nodeType === 1 ? node : node.parentElement;
        while (element && !element.id) { element = element.parentElement; }
        return element;
      }

      function offsetWithin(container, node, offset) {
        const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
        let total = 0;
        while (walker.nextNode()) {
          if (walker.currentNode === node) { return total + offset; }
          total += walker.currentNode.textContent.length;
        }
        return total;
      }

      document.addEventListener('selectionchange', () => {
        const selection = document.getSelection();
        if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
          post('selection', null);
          return;
        }
        const range = selection.getRangeAt(0);
        const block = blockFor(range.startContainer);
        if (!block) { post('selection', null); return; }
        post('selection', {
          text: selection.toString(),
          blockId: block.id,
          start: offsetWithin(block, range.startContainer, range.startOffset),
          end: offsetWithin(block, range.endContainer, range.endOffset)
        });
      });

      // ---- Marking (find hits and annotations) -----------------------------
      // Ranges are wrapped in <mark> and unwrapped again, so decoration never
      // disturbs the underlying document text.
      function clearMarks(className) {
        document.querySelectorAll('mark.' + className).forEach((mark) => {
          const parent = mark.parentNode;
          while (mark.firstChild) { parent.insertBefore(mark.firstChild, mark); }
          parent.removeChild(mark);
          parent.normalize();
        });
      }

      function markRange(block, start, end, className) {
        const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT);
        let index = 0;
        let startNode = null, startOffset = 0, endNode = null, endOffset = 0;
        while (walker.nextNode()) {
          const node = walker.currentNode;
          const length = node.textContent.length;
          if (!startNode && index + length > start) { startNode = node; startOffset = start - index; }
          if (!endNode && index + length >= end) { endNode = node; endOffset = end - index; break; }
          index += length;
        }
        if (!startNode || !endNode) return;
        try {
          const range = document.createRange();
          range.setStart(startNode, startOffset);
          range.setEnd(endNode, endOffset);
          const mark = document.createElement('mark');
          mark.className = className;
          range.surroundContents(mark);
        } catch (_) { /* range spans element boundaries; skip rather than corrupt */ }
      }

      bridge.applyAnnotations = function (annotations) {
        clearMarks('reader-annotation-highlight');
        clearMarks('reader-annotation-comment');
        (annotations || []).forEach((annotation) => {
          const block = document.getElementById(annotation.blockId);
          if (!block) return;
          markRange(block, annotation.start, annotation.end,
            annotation.kind === 'comment' ? 'reader-annotation-comment' : 'reader-annotation-highlight');
        });
      };

      // ---- Find ------------------------------------------------------------
      let findMatches = [];
      let findIndex = 0;

      bridge.find = function (query) {
        clearMarks('reader-find');
        clearMarks('reader-find-current');
        findMatches = [];
        findIndex = 0;
        const needle = (query || '').trim();
        if (!needle) { post('findResults', { count: 0, index: 0 }); return; }

        const lowered = needle.toLowerCase();
        const blocks = Array.from(document.querySelectorAll('[id]'));
        blocks.forEach((block) => {
          if (block.querySelector('[id]')) return; // only leaf blocks
          const text = block.textContent.toLowerCase();
          let from = 0;
          while (true) {
            const at = text.indexOf(lowered, from);
            if (at === -1) break;
            findMatches.push({ blockId: block.id, start: at, end: at + needle.length });
            from = at + needle.length;
          }
        });

        findMatches.forEach((match) => {
          const block = document.getElementById(match.blockId);
          if (block) { markRange(block, match.start, match.end, 'reader-find'); }
        });
        bridge.focusMatch(0);
        post('findResults', { count: findMatches.length, index: findMatches.length ? 1 : 0 });
      };

      bridge.focusMatch = function (index) {
        if (!findMatches.length) return;
        findIndex = ((index % findMatches.length) + findMatches.length) % findMatches.length;
        const marks = Array.from(document.querySelectorAll('mark.reader-find, mark.reader-find-current'));
        marks.forEach((mark, position) => {
          mark.className = position === findIndex ? 'reader-find-current' : 'reader-find';
        });
        const current = marks[findIndex];
        if (current) { current.scrollIntoView({ behavior: 'smooth', block: 'center' }); }
        post('findResults', { count: findMatches.length, index: findIndex + 1 });
      };

      bridge.findNext = function () { bridge.focusMatch(findIndex + 1); };
      bridge.findPrevious = function () { bridge.focusMatch(findIndex - 1); };

      window.__mdstar = bridge;
      reportActiveHeading();
      reportScrollOffset();
    })();
    """
}
