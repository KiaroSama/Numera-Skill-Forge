/* External AX client only: never evaluate JavaScript inside the application. */
ObjC.import('Foundation');
ObjC.import('ApplicationServices');
ObjC.bindFunction('AXIsProcessTrusted', ['bool', []]);
ObjC.bindFunction('AXUIElementCreateApplication', ['id', ['int']]);
ObjC.bindFunction('AXUIElementCopyAttributeValue', ['int', ['id', 'id', 'id*']]);
ObjC.bindFunction('AXUIElementCopyActionNames', ['int', ['id', 'id*']]);
ObjC.bindFunction('AXUIElementIsAttributeSettable', ['int', ['id', 'id', 'bool*']]);
ObjC.bindFunction('AXUIElementSetAttributeValue', ['int', ['id', 'id', 'id']]);
ObjC.bindFunction('AXUIElementPerformAction', ['int', ['id', 'id']]);
ObjC.bindFunction('AXUIElementSetMessagingTimeout', ['int', ['id', 'float']]);

function requireValue(condition, message) {
    if (!condition) throw new Error(message);
}
function scalar(value) {
    if (value === null || value === undefined) return null;
    try {
        var result = ObjC.unwrap(value);
        return typeof result === 'string' || typeof result === 'number' || typeof result === 'boolean' ? result : null;
    } catch (_) { return null; }
}
function attribute(element, name) {
    var out = Ref();
    var error = $.AXUIElementCopyAttributeValue(element, $(name), out);
    if (error === -25205 || error === -25212) return null; // Unsupported attribute or no value.
    if (error !== 0) {
        var failure = new Error('AX attribute ' + name + ' failed code=' + error);
        failure.axCode = error;
        failure.axAttribute = name;
        throw failure;
    }
    return out[0];
}
function timeout(element) {
    requireValue($.AXUIElementSetMessagingTimeout(element, 1) === 0, 'AX messaging timeout could not be set');
}
function text(element, name) {
    var value = scalar(attribute(element, name));
    return value === null ? '' : String(value);
}
function names(element) {
    var values = [text(element, 'AXTitle'), text(element, 'AXDescription'), text(element, 'AXHelp')];
    if (values.some(function (value) { return value.length > 0; })) return values.filter(function (value) { return value.length > 0; });
    if (['AXTextField', 'AXTextArea', 'AXCheckBox', 'AXPopUpButton', 'AXComboBox'].indexOf(text(element, 'AXRole')) < 0) return [];
    var label = attribute(element, 'AXTitleUIElement');
    if (label) {
        timeout(label);
        values.push(text(label, 'AXValue'), text(label, 'AXTitle'), text(label, 'AXDescription'));
    }
    return values.filter(function (value) { return value.length > 0; });
}
function tree(application) {
    var nodes = [], bytes = 0;
    function visit(element, depth, parent) {
        requireValue(depth <= 30 && nodes.length < 2000, 'AX traversal exceeded depth30/nodes2000');
        timeout(element);
        var node = { element: element, parent: parent, role: text(element, 'AXRole'), names: names(element), value: scalar(attribute(element, 'AXValue')) };
        bytes += JSON.stringify({role: node.role, names: node.names, value: node.value}).length;
        requireValue(bytes <= 250000, 'AX snapshot exceeds bounded text size');
        var position = nodes.length;
        nodes.push(node);
        var children = attribute(element, 'AXChildren');
        if (children) {
            var count = Number(children.count);
            requireValue(Number.isInteger(count) && count >= 0 && count <= 2000, 'Invalid AX child count');
            for (var index = 0; index < count; index++) visit(children.objectAtIndex(index), depth + 1, position);
        }
    }
    // The only roots are this PID's windows, never the global desktop or another app.
    var windows = attribute(application, 'AXWindows');
    if (!windows) return nodes;
    requireValue(Number(windows.count) <= 10, 'Owned application exposes too many windows');
    for (var index = 0; index < Number(windows.count); index++) visit(windows.objectAtIndex(index), 0, -1);
    return nodes;
}
function find(nodes, role, name) {
    var found = nodes.filter(function (node) { return node.role === role && node.names.indexOf(name) >= 0; });
    requireValue(found.length === 1, 'Expected one ' + role + ' named ' + name + '; found=' + found.length);
    return found[0].element;
}
function press(element) {
    var enabled = scalar(attribute(element, 'AXEnabled'));
    requireValue(enabled === true || enabled === 1, 'AX control is disabled or enabled state unavailable');
    var actions = Ref();
    requireValue($.AXUIElementCopyActionNames(element, actions) === 0, 'AX action inventory failed');
    var allowed = false;
    for (var index = 0; index < Number(actions[0].count); index++) {
        if (scalar(actions[0].objectAtIndex(index)) === 'AXPress') allowed = true;
    }
    requireValue(allowed, 'AXPress unsupported; no keyboard/focus fallback');
    requireValue($.AXUIElementPerformAction(element, $('AXPress')) === 0, 'AXPress failed');
}
function setValue(element, value) {
    var role = text(element, 'AXRole');
    if (role === 'AXTextField' || role === 'AXTextArea') {
        var focused = scalar(attribute(element, 'AXFocused'));
        if (focused !== true && focused !== 1) {
            var focusSettable = Ref();
            requireValue($.AXUIElementIsAttributeSettable(element, $('AXFocused'), focusSettable) === 0 && Boolean(focusSettable[0]), 'Owned text control cannot enter editing mode');
            requireValue($.AXUIElementSetAttributeValue(element, $('AXFocused'), $(true)) === 0, 'Owned text control focus failed');
            var focusDeadline = Date.now() + 2000;
            while (Date.now() < focusDeadline) {
                focused = scalar(attribute(element, 'AXFocused'));
                if (focused === true || focused === 1) break;
                $.NSThread.sleepForTimeInterval(Math.min(0.05, Math.max(0, (focusDeadline - Date.now()) / 1000)));
            }
            requireValue(focused === true || focused === 1, 'Owned text control editing readiness failed');
        }
    }
    var settable = Ref();
    requireValue($.AXUIElementIsAttributeSettable(element, $('AXValue'), settable) === 0 && Boolean(settable[0]), 'AXValue is not settable; no keyboard fallback');
    requireValue($.AXUIElementSetAttributeValue(element, $('AXValue'), $(value)) === 0, 'AXValue setter failed');
    // WebKit dispatches the mutation to its main thread; observe once, never replay the setter.
    var until = Date.now() + 5000, actual = null;
    while (Date.now() < until) {
        actual = scalar(attribute(element, 'AXValue'));
        if (actual === value) return;
        $.NSThread.sleepForTimeInterval(Math.min(0.05, Math.max(0, (until - Date.now()) / 1000)));
    }
    throw new Error('AXValue readback deadline exceeded actual_type=' + (actual === null ? 'null' : typeof actual) +
        ' actual_length=' + (typeof actual === 'string' ? actual.length : 'not-text') + ' expected_length=' + value.length);
}
function run(argv) {
    var environment = $.NSProcessInfo.processInfo.environment;
    requireValue(scalar(environment.objectForKey('GITHUB_ACTIONS')) === 'true' &&
        scalar(environment.objectForKey('RUNNER_ENVIRONMENT')) === 'github-hosted' &&
        scalar(environment.objectForKey('RUNNER_OS')) === 'macOS', 'AX helper is restricted to ephemeral GitHub-hosted CI');
    requireValue($.AXIsProcessTrusted(), 'Accessibility client is not trusted; no prompt or permission change permitted');
    if (argv.length === 1 && argv[0] === 'preflight') return JSON.stringify({trusted: true});
    requireValue(argv.length >= 2 && /^\d+$/.test(argv[0]) && Number(argv[0]) > 0, 'Supply owned positive PID and operation');
    var application = $.AXUIElementCreateApplication(Number(argv[0]));
    timeout(application);
    var operation = argv[1], nodes;
    try { nodes = tree(application); }
    catch (error) {
        // Only this read-side startup boundary is retryable; actions are never replayed.
        if (operation === 'snapshot' && ((error.axCode === -25204 && error.axAttribute === 'AXWindows') || error.axCode === -25202)) {
            return JSON.stringify({status: 'not-ready', ax_error: error.axCode, attribute: error.axAttribute});
        }
        throw error;
    }
    if (operation === 'snapshot') {
        return JSON.stringify({nodes: nodes.map(function (node) { return {parent: node.parent, role: node.role, names: node.names, value: node.value}; })});
    }
    requireValue(argv.length >= 4, 'Supply exact AX role and semantic name');
    var element = find(nodes, argv[2], argv[3]);
    if (operation === 'press') {
        requireValue(argv.length === 4, 'Invalid press arguments');
        press(element);
    } else if (operation === 'set') {
        requireValue(argv.length === 5, 'Invalid set arguments');
        setValue(element, argv[4]);
    } else if (operation === 'check') {
        requireValue(argv.length === 5 && (argv[4] === 'true' || argv[4] === 'false'), 'Invalid checkbox arguments');
        var before = scalar(attribute(element, 'AXValue'));
        requireValue(before === 0 || before === 1 || before === true || before === false, 'Checkbox state is not binary');
        if (Boolean(before) !== (argv[4] === 'true')) press(element);
        requireValue(Boolean(scalar(attribute(element, 'AXValue'))) === (argv[4] === 'true'), 'Checkbox did not retain selected state');
    } else if (operation === 'select') {
        requireValue(argv.length === 5, 'Invalid selection arguments');
        var writable = Ref();
        requireValue($.AXUIElementIsAttributeSettable(element, $('AXValue'), writable) === 0, 'Selection settable query failed');
        if (Boolean(writable[0])) setValue(element, argv[4]);
        else {
            press(element);
            press(find(tree(application), 'AXMenuItem', argv[4]));
        }
    } else throw new Error('Unsupported AX operation');
    return JSON.stringify({status: 'performed', operation: operation});
}
