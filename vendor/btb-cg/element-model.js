/**
 * ElementModel — jeden samowystarczalny byt: ELEMENT GRAFICZNY.
 *
 * Element niesie własny kod (html/css/js_template + ocgInputs) oraz własne wartości
 * (title, subtitle, fields, style, layout, animation, items ...). Nie ma osobnych
 * "szablonów" powiązanych przez templateId:
 *   - state.graphics — elementy w banku / na wizji,
 *   - state.library  — elementy-wzorce (ten sam kształt; "dodaj do banku" = kopia),
 *   - preset         — { graphics[], groups[] }.
 *
 * Moduł jest współdzielony: Node (server.js) i przeglądarka (window.ElementModel).
 * Zawiera też migrację ze starego modelu (szablon + grafika z templateId).
 */
(function (root, factory) {
    if (typeof module === 'object' && module.exports) module.exports = factory();
    else root.ElementModel = factory();
})(typeof self !== 'undefined' ? self : this, function () {

    const clone = (o) => (o === undefined ? undefined : JSON.parse(JSON.stringify(o)));

    // Pola przenoszone z defaultFields na najwyższy poziom elementu (tak czyta je renderer)
    const TOP_LEVEL_FIELD_KEYS = ['title', 'titleHtml', 'subtitle', 'items', 'speed', 'squashing', 'wiper'];
    const LEGACY_DEFAULT_KEYS = ['defaultFields', 'defaultStyle', 'defaultLayout', 'defaultAnimation'];
    const LEGACY_CODE_KEYS = ['templateId', 'useCodeOverride', 'html_override', 'css_override', 'js_override'];

    // Nazwy zmiennych kontekstu renderera (TITLE, SUBTITLE, LOGO_URL, ...) — wartość w fields je przykrywa
    const RESERVED_NAME = /^[A-Z][A-Z0-9_]*$/;

    const DEFAULT_ANIMATION = () => ({
        in:  { type: 'slide', direction: 'left', duration: 0.5, delay: 0, ease: 'ease-out' },
        out: { type: 'fade',  direction: 'left', duration: 0.5, delay: 0, ease: 'ease-in' }
    });

    function defaultStyle(df) {
        return {
            background: {
                type: 'solid',
                color: df.primaryColor || '#0047AB',
                color2: df.secondaryColor || '#000000',
                gradientAngle: 135,
                opacity: 1,
                borderColor: df.accentColor || '#ffffff',
                borderWidth: 0,
                borderRadius: 0,
                subtitleBackgroundColor: df.subtitleBackgroundColor || '#000000'
            },
            typography: {
                color: df.titleColor || '#ffffff',
                fontFamily: 'Inter',
                fontSize: df.titleSize || 30,
                fontWeight: df.titleWeight || 'bold'
            },
            subtitleTypography: {
                color: df.subtitleColor || '#eeeeee',
                fontFamily: 'Inter',
                fontSize: df.subtitleSize || 20,
                fontWeight: df.subtitleWeight || 'normal'
            }
        };
    }

    /**
     * Stary szablon → element (wzorzec do biblioteki). Wartości domyślne szablonu
     * (default*) stają się zwykłymi, bieżącymi wartościami elementu.
     */
    function fromTemplate(tpl) {
        const df = clone(tpl.defaultFields) || {};
        const dl = clone(tpl.defaultLayout) || {};
        const rest = clone(tpl) || {};
        LEGACY_DEFAULT_KEYS.concat(LEGACY_CODE_KEYS).forEach(k => delete rest[k]);

        const fields = { ...df };
        TOP_LEVEL_FIELD_KEYS.concat(['tickerSpeed', 'useCodeOverride']).forEach(k => delete fields[k]);
        (tpl.ocgInputs || []).forEach(inp => {
            if (fields[inp.id] !== undefined) return;
            // Pusty TITLE/SUBTITLE/... w fields przykryłby title/subtitle elementu przy renderowaniu
            if (RESERVED_NAME.test(inp.id) && !inp.default) return;
            fields[inp.id] = inp.default || '';
        });

        const el = {
            ...rest,
            id: tpl.id,
            name: tpl.name || 'Element',
            type: tpl.type || 'LOWER_THIRD',
            visible: false,
            title: df.title || '',
            subtitle: df.subtitle || '',
            items: df.items ? clone(df.items) : [],
            speed: df.speed !== undefined ? df.speed : (df.tickerSpeed !== undefined ? df.tickerSpeed : 100),
            squashing: df.squashing !== undefined ? df.squashing : false,
            fields,
            layout: {
                x: 100, y: 800, scale: 1, layer: 1,
                width: dl.width || df['f-width'] || df.width || undefined,
                height: dl.height || df['f-height'] || df.height || undefined,
                ...dl
            },
            animation: clone(tpl.defaultAnimation) || DEFAULT_ANIMATION(),
            style: clone(tpl.defaultStyle) || defaultStyle(df),
            hotkey: null
        };
        if (df.titleHtml !== undefined) el.titleHtml = df.titleHtml;
        if (df.wiper) el.wiper = clone(df.wiper);
        if (df.introText !== undefined) el.introText = df.introText;
        if (df.logoUrl !== undefined) el.url = df.logoUrl;
        if (df.tickerMode !== undefined) el.tickerMode = df.tickerMode;
        return el;
    }

    /**
     * Stara grafika (z templateId / override) → samowystarczalny element.
     * Własne wartości grafiki wygrywają; braki uzupełnia szablon.
     */
    function inlineGraphic(g, tpl) {
        const base = tpl ? fromTemplate(tpl) : {};
        const out = { ...base, ...clone(g) };

        // kod: override grafiki wygrywa z kodem szablonu
        const ov = g.useCodeOverride;
        if (ov && g.html_override != null) out.html_template = g.html_override;
        if (ov && g.css_override != null) out.css_template = g.css_override;
        if (ov && g.js_override != null) out.js_template = g.js_override;

        // wartości zagnieżdżone: scalaj sekcjami, żeby nie tracić domyślnych kluczy szablonu
        // fields: tylko własne wartości grafiki — szablon nigdy nie wstrzykiwał ich do kontekstu renderera
        out.fields = clone(g.fields) || {};
        if (base.style) {
            const gs = g.style || {};
            out.style = { ...base.style, ...clone(gs) };
            ['background', 'typography', 'subtitleTypography'].forEach(k => {
                if (base.style[k] || gs[k]) out.style[k] = { ...(base.style[k] || {}), ...(gs[k] || {}) };
            });
        }
        if (g.layout) {
            // x/y/side zostają takie, jakie miała grafika; szablon dokłada tylko wymiary
            out.layout = { ...clone(g.layout) };
            if (out.layout.width === undefined && base.layout?.width !== undefined) out.layout.width = base.layout.width;
            if (out.layout.height === undefined && base.layout?.height !== undefined) out.layout.height = base.layout.height;
        }
        if (g.animation) out.animation = clone(g.animation);

        out.id = g.id;
        out.visible = !!g.visible;
        LEGACY_CODE_KEYS.concat(LEGACY_DEFAULT_KEYS).forEach(k => delete out[k]);
        return out;
    }

    const isLegacyGraphic = (g) => !!(g && (g.templateId || g.useCodeOverride || g.html_override != null || g.css_override != null || g.js_override != null));

    /** Lista grafik → lista samowystarczalnych elementów. Zwraca { graphics, changed }. */
    function inlineGraphics(graphics, templatesById) {
        let changed = false;
        const sourceIds = [];
        const out = (graphics || []).map(g => {
            if (!isLegacyGraphic(g)) return g;
            changed = true;
            if (g.templateId) sourceIds.push(g.templateId);
            return inlineGraphic(g, templatesById.get(g.templateId));
        });
        return { graphics: out, changed, sourceIds };
    }

    /**
     * Migracja całego stanu (in place). Rozpoznaje stary format po `templates`
     * oraz grafikach z templateId. Zwraca true, jeśli coś zmieniono.
     */
    function migrateState(state, fallbackTemplates) {
        if (!state) return false;
        let changed = false;
        const legacyTemplates = Array.isArray(state.templates) ? state.templates : [];
        const byId = new Map(legacyTemplates.filter(t => t && t.id).map(t => [t.id, t]));

        // biblioteka = dawne szablony (tylko jeśli biblioteki jeszcze nie ma)
        if (!Array.isArray(state.library)) state.library = [];
        if (state.library.length === 0 && legacyTemplates.length > 0) {
            state.library = legacyTemplates.filter(t => t && t.id).map(fromTemplate);
            changed = true;
        }
        // szablony nadal potrzebne do inline'owania grafik, które nie mają jeszcze kodu
        state.library.forEach(item => { if (!byId.has(item.id)) byId.set(item.id, item); });
        if ('templates' in state) { delete state.templates; changed = true; }

        // Grafiki "osierocone" (szablon źródłowy usunięty wcześniej) odzyskujemy ze źródeł zapasowych:
        // szablonów dołączonych do presetów oraz plików z katalogu templates/.
        const fallback = new Map();
        const addFallback = (t) => { if (t && t.id && !byId.has(t.id) && !fallback.has(t.id)) fallback.set(t.id, t); };
        (fallbackTemplates || []).forEach(addFallback);
        (state.presets || []).forEach(p => (p.templates || []).forEach(addFallback));
        const allById = new Map([...fallback, ...byId]);

        const g = inlineGraphics(state.graphics, allById);
        state.graphics = g.graphics;
        changed = changed || g.changed;

        // odzyskane szablony wracają do biblioteki
        const usedFallback = new Set();
        (g.sourceIds || []).forEach(id => { if (fallback.has(id)) usedFallback.add(id); });
        usedFallback.forEach(id => state.library.push(fromTemplate(fallback.get(id))));

        if (Array.isArray(state.presets)) {
            state.presets.forEach(p => {
                const pById = new Map(allById);
                (p.templates || []).forEach(t => { if (t && t.id) pById.set(t.id, t); });
                const r = inlineGraphics(p.graphics, pById);
                p.graphics = r.graphics;
                if (r.changed) changed = true;
                if ('templates' in p) { delete p.templates; changed = true; }
            });
        }

        // naprawa pól, które przykrywały tytuł (patrz repairFields)
        const lists = [state.graphics, state.library, ...(state.presets || []).map(p => p.graphics)];
        lists.forEach(list => (list || []).forEach(el => {
            if (repairFields(el)) changed = true;
            if (migrateLegacyScene(el)) changed = true;
            if (refreshScene(el)) changed = true;
        }));
        return changed;
    }

    /**
     * Usuwa z fields wpisy, które przykrywały tytuł/podtytuł elementu (artefakt wcześniejszej migracji):
     * puste zmienne ALL_CAPS oraz TITLE/SUBTITLE równe wartości domyślnej, gdy element ma własny tekst.
     * Zwraca true, jeśli coś usunięto.
     */
    function repairFields(el) {
        if (!el || !el.fields || typeof el.fields !== 'object') return false;
        const ids = new Map((el.ocgInputs || []).map(i => [i.id, i.default]));
        let changed = false;
        Object.keys(el.fields).forEach(k => {
            if (!RESERVED_NAME.test(k) || !ids.has(k)) return;
            const v = el.fields[k];
            const hasOwnTitle = k === 'TITLE' && (el.title || el.titleHtml);
            const hasOwnSub = k === 'SUBTITLE' && el.subtitle;
            const isInjectedDefault = (hasOwnTitle || hasOwnSub) && v === ids.get(k);
            if (v === '' || v == null || isInjectedDefault) { delete el.fields[k]; changed = true; }
        });
        return changed;
    }

    /**
     * Stare pole _sceneData (scene_designer: warstwy w px płótna 1920×1080) → element.scene (SceneModel v1).
     * Kod elementu jest kompilowany od nowa, a pozycja płótna przechodzi do layout (side:'custom').
     * Brak modułu SceneModel (np. okrojony kontekst) = pomijamy, pole zostaje.
     */
    function migrateLegacyScene(el) {
        if (!el || !el._sceneData) return false;
        const SM = sceneModel();
        if (!SM) return false;
        const r = SM.fromLegacyScene(el._sceneData);
        delete el._sceneData;
        if (!r) return true;
        SM.applyToElement(el, r.scene, { resetValues: false });
        el.layout = { ...(el.layout || {}), side: 'custom', x: (Number(el.layout?.x) || 0) + r.origin.x, y: (Number(el.layout?.y) || 0) + r.origin.y };
        el.animation = { ...(el.animation || {}), in: { ...(el.animation?.in || {}), ...r.animation } };
        return true;
    }

    /**
     * Sceny skompilowane wcześniejszą wersją kompilatora miały selektor `.cg-lN > .cg-i`, którego renderer
     * nie potrafi zescopować (brak stylów na wizji). Kompilujemy je od nowa z zapisanej sceny.
     */
    function refreshScene(el) {
        if (!el || !el.scene || el.scene.detached || !el.scene.layers) return false;
        if (!/\.cg-l\d+\s*>/.test(el.css_template || '')) return false;
        const SM = sceneModel();
        if (!SM) return false;
        SM.applyToElement(el, el.scene, { resetValues: false });
        return true;
    }

    function sceneModel() {
        try {
            if (typeof module === 'object' && module.exports) return require('./scene-model');
        } catch (e) { /* brak modułu */ }
        return (typeof self !== 'undefined' && self.SceneModel) || null;
    }

    /**
     * Ustawia wyświetlany tytuł elementu. Renderer wybiera tytuł w kolejności:
     * fields.TITLE > titleHtml > titleLines > title — samo `title` bywa więc ignorowane.
     * Dlatego każda edycja tekstu przechodzi tu: zapisuje `title` i usuwa przesłaniające warianty.
     * Jeśli tytuł ma formatowanie (titleHtml z jednym fragmentem tekstu), formatowanie zostaje, a podmieniany jest tekst.
     */
    function setTitle(el, text) {
        if (!el) return el;
        const value = String(text ?? '');
        let html = null;
        if (el.titleHtml && typeof DOMParser !== 'undefined') {
            try {
                const doc = new DOMParser().parseFromString('<div id="r">' + el.titleHtml + '</div>', 'text/html');
                const rootEl = doc.getElementById('r');
                const walker = doc.createTreeWalker(rootEl, 4 /* SHOW_TEXT */);
                const nodes = [];
                while (walker.nextNode()) { if (walker.currentNode.nodeValue.trim() !== '') nodes.push(walker.currentNode); }
                if (nodes.length === 1) {
                    nodes[0].nodeValue = value;
                    html = rootEl.innerHTML;
                }
            } catch (e) { html = null; }
        }
        el.title = value;
        if (html !== null && value !== '') el.titleHtml = html; else delete el.titleHtml;
        delete el.titleLines;
        if (el.fields) delete el.fields.TITLE;
        return el;
    }

    function setSubtitle(el, text) {
        if (!el) return el;
        el.subtitle = String(text ?? '');
        if (el.fields) delete el.fields.SUBTITLE;
        return el;
    }

    const htmlToText = (html) => {
        const s = String(html ?? '');
        if (typeof document !== 'undefined') { const d = document.createElement('div'); d.innerHTML = s; return d.textContent || ''; }
        return s.replace(/<[^>]+>/g, '');
    };
    const hasMarkup = (html) => /<[a-z][\s\S]*>/i.test(String(html ?? ''));

    // Utrzymuje spójność domyślnego tekstu warstw sceny z wartością elementu
    function syncSceneDefault(el, field, text) {
        if (!el || !el.scene || !Array.isArray(el.scene.layers)) return;
        el.scene.layers.forEach(l => { if (l.type === 'text' && l.field === field) l.defaultText = text; });
    }

    // ── Integracja Edytora Tekstu (WYSIWYG) z zakładką Typografia ──────────────
    // Właściwości inline, którymi steruje też zakładka Typografia (style.typography / subtitleTypography).
    // Inne marki WYSIWYG (podkreślenie, highlight, odstępy, transformacja, wyrównanie) nie mają odpowiednika
    // w inspektorze i zawsze zostają jako formatowanie lokalne tekstu.
    const TEXT_STYLE_PROPS = ['fontFamily', 'fontSize', 'fontWeight', 'color'];

    // Pierwszy nadrzędny element (do rootEl) ustawiający dany inline-style dla danego węzła tekstu.
    function _resolveInlineStyleAt(node, rootEl, prop) {
        let n = node.parentElement;
        while (n && n !== rootEl) {
            if (n.style && n.style[prop]) return n.style[prop];
            n = n.parentElement;
        }
        return null;
    }

    // Usuwa dany zestaw inline-style z wszystkich elementów poddrzewa i odwija spany, które
    // po usunięciu nie mają już żadnych atrybutów (czyszczenie po `normalizeHtml`-owemu).
    function _stripInlineStyleProps(rootEl, props) {
        Array.from(rootEl.querySelectorAll('*')).forEach(node => {
            if (!node.style) return;
            props.forEach(p => { node.style[p] = ''; });
            if (node.getAttribute('style') === '') node.removeAttribute('style');
        });
        let changed = true;
        while (changed) {
            changed = false;
            Array.from(rootEl.querySelectorAll('span')).forEach(span => {
                if (span.attributes.length === 0) {
                    const parent = span.parentNode;
                    if (!parent) return;
                    while (span.firstChild) parent.insertBefore(span.firstChild, span);
                    parent.removeChild(span);
                    changed = true;
                }
            });
        }
    }

    // Jeśli CAŁY widoczny tekst niesie tę samą wartość danej właściwości (fontFamily/fontSize/
    // fontWeight/color), "promuje" ją — zwraca ją do zapisania w style.typography i usuwa odpowiadający
    // inline-style z HTML, żeby zakładka Typografia odzyskała pełną kontrolę. Właściwości ustawione
    // tylko na części tekstu (formatowanie mieszane) zostają nietknięte — nie da się ich wyrazić jedną
    // wartością w inspektorze.
    function _promoteUniformInlineStyle(html, props) {
        const result = { html, extracted: {} };
        if (!html || typeof DOMParser === 'undefined' || !hasMarkup(html)) return result;
        try {
            const doc = new DOMParser().parseFromString('<div id="r">' + html + '</div>', 'text/html');
            const rootEl = doc.getElementById('r');
            const walker = doc.createTreeWalker(rootEl, 4 /* SHOW_TEXT */);
            const textNodes = [];
            while (walker.nextNode()) { if (walker.currentNode.nodeValue.trim() !== '') textNodes.push(walker.currentNode); }
            if (!textNodes.length) return result;

            const uniformProps = [];
            props.forEach(prop => {
                const values = textNodes.map(n => _resolveInlineStyleAt(n, rootEl, prop));
                const first = values[0];
                if (first && values.every(v => v === first)) {
                    uniformProps.push(prop);
                    result.extracted[prop] = first;
                }
            });
            if (!uniformProps.length) return result;

            _stripInlineStyleProps(rootEl, uniformProps);
            result.html = rootEl.innerHTML;
        } catch (e) { /* zostaw HTML bez zmian */ }
        return result;
    }

    // Czy w HTML pola zostało formatowanie WYSIWYG (font/rozmiar/kolor), którego zakładka Typografia
    // nie obejmuje w całości — bo nie jest jednolite na całym tekście (inaczej zostałoby już "promowane"
    // przy ostatnim zapisie przez setRichText).
    function _hasInlineStyle(html, props) {
        if (!html || typeof DOMParser === 'undefined' || !hasMarkup(html)) return false;
        try {
            const doc = new DOMParser().parseFromString('<div id="r">' + html + '</div>', 'text/html');
            const rootEl = doc.getElementById('r');
            return Array.from(rootEl.querySelectorAll('*')).some(node => node.style && props.some(p => !!node.style[p]));
        } catch (e) { return false; }
    }

    // Właściwości tekstu, które KOD elementu faktycznie czyta z style.typography (zmienne szablonu). Elementy ze sceną i inne bez tych
    // zmiennych nie konsumują typografii — formatowanie z Edytora Tekstu (kolor, krój, rozmiar, grubość) musi zostać w HTML pola,
    // bo „promowanie” do typografii kasowało je z tytułu, a nic go potem nie stosowało na wizji.
    const TYPO_VARS = {
        TITLE: { fontFamily: ['FONT_FAMILY', 'TITLE_FONT'], fontSize: ['TITLE_SIZE'], fontWeight: ['TITLE_WEIGHT'], color: ['TITLE_COLOR'] },
        SUBTITLE: { fontFamily: ['FONT_FAMILY', 'TITLE_FONT'], fontSize: ['SUBTITLE_SIZE'], fontWeight: [], color: ['SUBTITLE_COLOR'] },
    };
    function _promotableProps(el, field) {
        const map = TYPO_VARS[field] || TYPO_VARS.TITLE;
        const code = (el.html_template || '') + '\n' + (el.css_template || '') + '\n' + (el.js_template || '');
        const used = new Set();
        for (const m of code.matchAll(/\{\{\{?\s*(?:#if\s+|#unless\s+)?([A-Z_][A-Z0-9_]*)/g)) used.add(m[1]);
        return TEXT_STYLE_PROPS.filter(p => (map[p] || []).some(v => used.has(v)));
    }

    function _applyExtractedTypography(el, field, extracted) {
        if (!extracted || !Object.keys(extracted).length) return;
        const key = field === 'SUBTITLE' ? 'subtitleTypography' : 'typography';
        el.style = el.style || {};
        const t = el.style[key] = el.style[key] || {};
        if (extracted.fontFamily) t.fontFamily = String(extracted.fontFamily).split(',')[0].replace(/['"]/g, '').trim();
        if (extracted.fontSize) { const n = parseFloat(extracted.fontSize); if (!isNaN(n)) t.fontSize = Math.round(n); }
        if (extracted.fontWeight) t.fontWeight = String(extracted.fontWeight).trim();
        if (extracted.color) t.color = extracted.color;
    }

    // Czy pole TITLE/SUBTITLE ma formatowanie WYSIWYG (font/rozmiar/kolor), które jest "mieszane" —
    // obejmuje tylko część tekstu i nie da się go pokazać/edytować jedną wartością w zakładce Typografia.
    function hasMixedTextStyle(el, field) {
        return _hasInlineStyle(getRichText(el, field), TEXT_STYLE_PROPS);
    }

    // Usuwa z pola TITLE/SUBTITLE całe lokalne formatowanie font/rozmiar/kolor z Edytora Tekstu — tekst
    // wraca pod pełną kontrolę zakładki Typografia (inne marki — pogrubienie, podkreślenie, highlight,
    // wyrównanie — zostają).
    function clearTextStyleOverrides(el, field) {
        const html = getRichText(el, field);
        if (!html || typeof DOMParser === 'undefined' || !hasMarkup(html)) return el;
        try {
            const doc = new DOMParser().parseFromString('<div id="r">' + html + '</div>', 'text/html');
            const rootEl = doc.getElementById('r');
            _stripInlineStyleProps(rootEl, TEXT_STYLE_PROPS);
            setRichText(el, field, rootEl.innerHTML);
        } catch (e) { /* noop */ }
        return el;
    }

    /**
     * Zapis pola tekstowego z Edytora Tekstu (HTML z TipTap).
     * field: 'TITLE' | 'SUBTITLE' | inne pole z ocgInputs | null → items tickera.
     */
    function setRichText(el, field, html) {
        if (!el) return el;
        if (field === 'TITLE' || field === 'SUBTITLE') {
            // Formatowanie jednolite na całym tekście (np. operator zmienił font/rozmiar/kolor z
            // zaznaczonym całym tytułem) trafia do style.typography/subtitleTypography — tak, żeby
            // zakładka Typografia nadal nad nim panowała; usuwamy wtedy odpowiadający inline-style.
            const promotable = _promotableProps(el, field);
            if (promotable.length) {
                const promoted = _promoteUniformInlineStyle(html, promotable);
                _applyExtractedTypography(el, field, promoted.extracted);
                html = promoted.html;
            }
        }
        const text = htmlToText(html);
        if (field === 'TITLE') {
            setTitle(el, text);                       // czyści titleHtml/titleLines/fields.TITLE
            if (hasMarkup(html)) el.titleHtml = html;
            syncSceneDefault(el, 'TITLE', text);
        } else if (field === 'SUBTITLE') {
            setSubtitle(el, text);                    // czyści fields.SUBTITLE
            el.fields = el.fields || {};
            if (hasMarkup(html)) el.fields.SUBTITLE = html;   // renderer: fields.SUBTITLE przesłania subtitle
            syncSceneDefault(el, 'SUBTITLE', text);
        } else if (field) {
            el.fields = el.fields || {};
            el.fields[field] = html;
            syncSceneDefault(el, field, text);
        } else {
            const raw = String(html ?? '').split(/<br\s*\/?>|<\/?div>|<\/?p>/i);
            el.items = raw.filter(x => x.replace(/&nbsp;/g, '').trim() !== '');
        }
        return el;
    }

    /** Początkowy HTML dla Edytora Tekstu. */
    function getRichText(el, field) {
        if (!el) return '';
        if (field === 'TITLE') {
            if (el.fields && el.fields.TITLE) return el.fields.TITLE;
            if (el.titleHtml) return el.titleHtml;
            if (el.titleLines && el.titleLines.length) {
                return el.titleLines.map(l => '<div style="font-size:' + (l.fontSize || 48) + 'px;font-weight:' + (l.fontWeight || '800') +
                    ';color:' + (l.color || '#fff') + ';text-transform:' + (l.textTransform || 'uppercase') + '">' + l.text + '</div>').join('');
            }
            return el.title || '';
        }
        if (field === 'SUBTITLE') return (el.fields && el.fields.SUBTITLE) || el.subtitle || '';
        if (field) return (el.fields && el.fields[field]) || '';
        return (el.items || []).map(i => (i && typeof i === 'object') ? (i.text || '') : String(i)).join('<br>');
    }

    /** Kopia elementu jako nowa instancja (np. z biblioteki do banku). */
    function instantiate(item, newId) {
        const el = clone(item);
        el.id = newId;
        el.visible = false;
        delete el.groupId;
        delete el._codeTab;
        return el;
    }

    return { fromTemplate, inlineGraphic, inlineGraphics, migrateState, repairFields, migrateLegacyScene, refreshScene, setTitle, setSubtitle, setRichText, getRichText, hasMixedTextStyle, clearTextStyleOverrides, instantiate, isLegacyGraphic, clone };
});
