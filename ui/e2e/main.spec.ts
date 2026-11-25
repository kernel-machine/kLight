import { expect, test, type Locator, type Page } from '@playwright/test';

async function runtime(page: Page) {
  const r = await page.request.get('/api/runtime');
  return r.json();
}

test('patch, program, blackout and grand master', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();

  // Patch two dimmers from the library, picked by manufacturer then model.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');
  const addressField = page.getByLabel('Address');
  await expect(addressField).toHaveValue('1');
  await page.getByLabel('Count').fill('2');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  await expect(page.locator('.tree li', { hasText: 'Dimmer 2' })).toBeVisible();
  // The next suggestion is the first free address after the patched dimmers.
  await expect(addressField).toHaveValue('3');

  // Several multi-channel fixtures land one after the other.
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await expect(addressField).toHaveValue('3');
  // The channels of the selected mode are listed.
  const channels = page.getByLabel('Mode channels').locator('tbody tr');
  await expect(channels).toHaveCount(11);
  await expect(channels.nth(1)).toContainText('Pan Fine');
  await page.getByLabel('Count').fill('3');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  for (const [n, range] of [[1, '3–13'], [2, '14–24'], [3, '25–35']] as const) {
    await expect(page.locator('.tree li', { hasText: `Spot 540 ${n}` })).toContainText(range);
  }
  await expect(addressField).toHaveValue('36');
  await page.getByLabel('Count').fill('1');

  // QLC+ definitions are in the library, sorted by brand (when downloaded:
  // scripts/update-qlcplus-fixtures.sh; CI always downloads them).
  const library = (await (await page.request.get('/api/library')).json()) as Array<{ manufacturer: string }>;
  if (library.some((e) => e.manufacturer === 'Martin')) {
    await page.getByPlaceholder('Search manufacturer or model…').fill('Martin');
    await expect(page.getByLabel('Manufacturer').locator('option').first()).toContainText('Martin');
    await page.getByPlaceholder('Search manufacturer or model…').fill('');
  } else {
    console.warn('QLC+ library not downloaded: brand search not tested');
  }
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');

  // A typed conflicting address is flagged before sending.
  await addressField.fill('1');
  await expect(page.getByText(/Conflicts with/)).toBeVisible();
  await page.getByRole('button', { name: 'First free' }).click();
  await expect(addressField).toHaveValue('36');

  // Simple Desk: select a fixture and set it to full.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'Dimmer 1', exact: true }).click();
  await page.getByRole('button', { name: 'Full' }).click();
  await expect(page.getByText('Intensity 100%')).toBeVisible();
  await expect.poll(async () => (await runtime(page)).state.programmer.length).toBe(1);

  // Save the programmer as a scene: it appears in the FX screen.
  page.once('dialog', (d) => d.accept('Look 1'));
  await page.getByRole('button', { name: 'Save as scene' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await expect(page.getByText('Look 1')).toBeVisible();

  // Blackout and grand master are always reachable.
  await page.getByRole('button', { name: 'Blackout' }).click();
  await expect(page.getByRole('button', { name: 'BLACKOUT ON' })).toBeVisible();
  await expect.poll(async () => (await runtime(page)).state.blackout).toBe(true);
  await page.getByRole('button', { name: 'BLACKOUT ON' }).click();
  await expect.poll(async () => (await runtime(page)).state.blackout).toBe(false);
});

test('scene editor, movement preview, knob and DMX channel desk', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();

  // Select heads in the universe tree and create a group.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Select Dimmer 1').check();
  await page.getByLabel('Select Dimmer 2').check();
  page.once('dialog', (d) => d.accept('Front'));
  await page.getByRole('button', { name: 'Create group' }).click();
  // The group is a folder of the universe tree holding its fixtures, and
  // opens its editor.
  const front = page.getByRole('treeitem', { name: 'Group Front' });
  await expect(front).toBeVisible();
  await expect(front).toHaveAttribute('aria-selected', 'true');
  await expect(front).toContainText('2');

  // Reorder the group by dragging heads in the matrix.
  await expect(page.getByLabel('Head Dimmer 1')).toContainText('1');
  await page.getByLabel('Head Dimmer 1').dragTo(page.getByLabel('Head Dimmer 2'));
  await expect(page.getByLabel('Head Dimmer 2')).toContainText('1');
  await expect(page.getByLabel('Head Dimmer 1')).toContainText('2');

  // Settings depend on the fixture type: a dimmer has no orientation.
  await page.locator('.tree li', { hasText: 'Dimmer 1' }).first().locator('button.fx').click();
  await expect(page.getByRole('heading', { name: 'Channels' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Orientation' })).toHaveCount(0);

  // A second group, of moving heads.
  await page.getByLabel('Select Spot 540 1').check();
  await page.getByLabel('Select Spot 540 2').check();
  page.once('dialog', (d) => d.accept('Beams'));
  await page.getByRole('button', { name: 'Create group' }).click();
  await expect(page.getByRole('treeitem', { name: 'Group Beams' })).toBeVisible();

  // Add a group to a scene from the scene editor.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await page.getByLabel('Scene target').selectOption({ label: 'Group: Front' });
  await page.getByLabel('Scene attribute').selectOption('intensity');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(page.locator('td', { hasText: 'Intensity 100%' })).toBeVisible();

  // Only what the fixtures can do is offered: dimmers have no zoom, strobe,
  // position or color; moving heads do.
  const attribute = page.getByLabel('Scene attribute');
  await expect(attribute.locator('option')).toHaveText(['Intensity']);
  await page.getByLabel('Scene target').selectOption({ label: 'Group: Beams' });
  await expect(attribute.locator('option')).toHaveText([
    'Intensity', 'Color', 'Position', 'Fan (group order)', 'Gobo', 'Prism', 'Zoom', 'Strobe',
  ]);

  // Regression: quick successive additions must all be kept (each used to
  // be computed on a stale copy of the scene and overwrite the previous one).
  await page.getByLabel('Scene attribute').selectOption('zoom');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await page.getByLabel('Scene attribute').selectOption('strobe');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await page.getByLabel('Scene attribute').selectOption('position');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  for (const text of ['Intensity 100%', 'Zoom 20°', 'Strobe 5 Hz', 'Pan 0.0° Tilt 0.0°']) {
    await expect(page.locator('td', { hasText: text })).toBeVisible();
  }

  // Below, one row of faders with every channel of the fixtures it drives.
  const channels2 = page.getByRole('group', { name: 'Scene channels' });
  await expect(page.getByText('Fixtures in this scene (4)')).toBeVisible();
  const dimmer = channels2.getByLabel('Dimmer 1: Intensity value', { exact: true });
  await expect(dimmer).toHaveValue('255');
  await page.getByRole('treeitem', { name: /Scene 2/ }).getByRole('button', { name: 'Start' }).click();
  await expect(channels2.locator('.strip', { has: page.getByLabel('Dimmer 1: Intensity value', { exact: true }) }).locator('.now')).toHaveText('255');

  // A fader moved by hand is stored in the scene (the summary row comes from
  // the show the engine sends back). Dimmer 2: nothing else holds its output
  // (the programmer still has Dimmer 1 at full).
  const byHandFader = channels2.getByLabel('Dimmer 2: Intensity value', { exact: true });
  const strip = channels2.locator('.strip', { has: page.getByLabel('Dimmer 2: Intensity value', { exact: true }) });
  const byHand = page.getByText('1 channel set by hand');
  await byHandFader.fill('100');
  await byHandFader.press('Enter');
  await expect(byHand).toBeVisible();
  await expect(strip).toHaveClass(/\bset\b/);
  await expect(strip.locator('.now')).toHaveText('100');
  await channels2.getByRole('slider', { name: 'Dimmer 2: Intensity', exact: true }).fill('90');
  await expect(strip.locator('.now')).toHaveText('90');
  await expect(byHandFader).toHaveValue('90');
  // Releasing gives the channel back to the values above.
  await channels2.getByRole('button', { name: 'Release Dimmer 2: Intensity', exact: true }).click();
  await expect(byHand).toHaveCount(0);
  await expect(byHandFader).toHaveValue('255');
  // Applying a value again releases the channels it writes.
  await byHandFader.fill('100');
  await byHandFader.press('Enter');
  await expect(byHand).toBeVisible();
  await page.getByLabel('Scene target').selectOption({ label: 'Group: Front' });
  await page.getByLabel('Scene attribute').selectOption('intensity');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(byHand).toHaveCount(0);
  await expect(byHandFader).toHaveValue('255');
  await page.getByLabel('Scene target').selectOption({ label: 'Group: Beams' });

  await expect.poll(async () => {
    const show = await (await page.request.get('/api/runtime')).json();
    return show.seq;
  }).toBeGreaterThan(0);

  // A fan on a group is an intention resolved by the core.
  await page.getByLabel('Scene attribute').selectOption('fan');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(page.locator('td', { hasText: /^Fan 60° linear/ })).toBeVisible();

  // Movement effect: the drawing follows the values.
  await page.getByRole('button', { name: 'New movement', exact: true }).click();
  const shape = page.locator('svg[aria-label="Movement preview"] path');
  await expect(shape).toBeVisible();
  const circle = await shape.getAttribute('d');
  await page.getByRole('button', { name: 'eight' }).click();
  await expect.poll(() => shape.getAttribute('d')).not.toBe(circle);
  const before = await shape.getAttribute('d');
  await page.locator('label.slider', { hasText: 'Width' }).locator('input[type=number]').fill('200');
  await page.locator('label.slider', { hasText: 'Width' }).locator('input[type=number]').press('Enter');
  await expect.poll(() => shape.getAttribute('d')).not.toBe(before);

  // Simple Desk: every channel of the universe can be set on its own.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'DMX channels' }).click();
  await expect(page.getByLabel('Channel 512', { exact: true })).toBeAttached();
  await page.getByLabel('Channel 300 value').fill('77');
  await page.getByLabel('Channel 300 value').press('Enter');
  await expect
    .poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels)
    .toContainEqual({ universe: 1, address: 300, value: 77 });
  await page.getByRole('button', { name: 'Release universe' }).click();
  await expect.poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels).toEqual([]);

  // Virtual console: a knob is a rotary control.
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.getByRole('button', { name: 'Knob' }).click();
  await expect(page.getByRole('slider', { name: 'Knob' })).toBeVisible();

  // Matrix effect preview shows one cell per head of the group.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New matrix effect', exact: true }).click();
  await expect(page.locator('.matrix .cell')).toHaveCount(2);

  // DMX monitor names fixtures and channels.
  await page.getByRole('button', { name: 'DMX monitor' }).click();
  await expect(page.locator('.monitor .cell').first()).toContainText('Dimmer 1');
  await page.getByRole('button', { name: 'Close' }).click();

  // Deleting a universe deletes its fixtures too.
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByRole('button', { name: '+ Universe' }).click();
  await expect(page.getByText('2: Universe 2')).toBeVisible();
  await page.getByLabel('Manufacturer').selectOption('Generic');
  await page.getByLabel('Model').selectOption('Generic/Dimmer');
  await page.locator('section', { hasText: 'Add fixtures' }).getByLabel('Universe').selectOption('2');
  await page.getByPlaceholder('Name (default: model)').fill('Far dimmer');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();
  await expect(page.locator('.tree li', { hasText: 'Far dimmer' })).toBeVisible();
  page.once('dialog', (d) => d.accept());
  await page.getByLabel('Delete universe Universe 2').click();
  await expect(page.getByText('2: Universe 2')).toHaveCount(0);
  await expect(page.locator('.tree li', { hasText: 'Far dimmer' })).toHaveCount(0);
});

test('functions are listed by type with nestable folders', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const tree = page.getByRole('tree');
  await expect(tree.getByText('Scenes', { exact: true })).toBeVisible();
  // Types without anything in them are not listed.
  await expect(tree.getByText('Chasers', { exact: true })).toHaveCount(0);
  await expect(tree.getByText('Cue lists', { exact: true })).toHaveCount(0);
  // A folder alone makes its type appear.
  page.once('dialog', (d) => d.accept('Songs'));
  await page.getByLabel('New folder in').selectOption('cue_list');
  await expect(tree.getByText('Cue lists', { exact: true })).toBeVisible();

  // A folder in Scenes, a subfolder inside it, and a new scene in the subfolder.
  page.once('dialog', (d) => d.accept('Act 1'));
  await page.getByRole('button', { name: 'New scene folder' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toBeVisible();
  page.once('dialog', (d) => d.accept('Song A'));
  await page.getByRole('button', { name: 'New subfolder in Act 1' }).click();
  await expect(page.getByLabel('Folder Song A', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'New function in Song A' }).click();
  const editorFolder = page.getByLabel('Folder', { exact: true });
  await expect(editorFolder.locator('option:checked')).toHaveText('Act 1 / Song A');

  // Regression: two creations in a row must not reuse the same id (the
  // second used to overwrite the first before the show update arrived).
  await page.getByRole('button', { name: 'New function in Act 1' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toContainText('2');
  await expect(page.getByLabel('Folder Song A', { exact: true })).toContainText('1');

  // Drag an existing scene ("Look 1", top level) into "Act 1".
  await tree.getByRole('treeitem', { name: /Look 1/ }).dragTo(page.getByLabel('Folder Act 1', { exact: true }));
  await tree.getByRole('treeitem', { name: /Look 1/ }).click();
  await expect(editorFolder.locator('option:checked')).toHaveText('Act 1');

  // Deleting "Act 1" moves its content up one level.
  page.once('dialog', (d) => d.accept());
  await page.getByRole('button', { name: 'Delete folder Act 1' }).click();
  await expect(page.getByLabel('Folder Act 1', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Folder Song A', { exact: true })).toBeVisible();
  await tree.getByRole('treeitem', { name: /Look 1/ }).click();
  await expect(editorFolder.locator('option:checked')).toHaveText('— top level —');
});


// Regressions: a dropped widget flashed back to its start for one round trip
// to the engine, and the clock widget never ticked.
test('virtual console: a dropped widget stays put and the clock ticks', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  const toolbar = page.locator('.toolbar');

  await toolbar.getByRole('button', { name: 'Clock', exact: true }).click();
  const clock = page.locator('.w.clock .big').last();
  const shown = await clock.textContent();
  await expect.poll(() => clock.textContent(), { timeout: 3000 }).not.toBe(shown);

  await toolbar.getByRole('button', { name: 'Label', exact: true }).click();
  const w = page.locator('.w.label').last();
  const styleLeft = () => w.evaluate((el) => parseFloat((el as HTMLElement).style.left));
  const start = await styleLeft();
  const box = (await w.boundingBox())!;
  await page.mouse.move(box.x + 5, box.y + 5);
  await page.mouse.down();
  await page.mouse.move(box.x + 85, box.y + 5, { steps: 5 });
  // Record where the widget is drawn on every frame from just before the drop.
  await w.evaluate((el) => {
    const seen: number[] = [];
    (window as unknown as { lefts: number[] }).lefts = seen;
    const frame = () => {
      seen.push(el.getBoundingClientRect().left);
      if (seen.length < 60) requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  });
  await page.mouse.up();
  await page.waitForFunction(() => (window as unknown as { lefts: number[] }).lefts.length >= 60);
  const lefts = await page.evaluate(() => (window as unknown as { lefts: number[] }).lefts);
  expect(Math.min(...lefts)).toBeGreaterThan(box.x + 40);
  await expect.poll(styleLeft).toBe(start + 80);
});

test('a channel with presets opens a list and the fader jumps to the chosen one', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();

  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'DMX channels' }).click();
  await page.getByRole('button', { name: 'Patched', exact: true }).click();

  // The color wheel (5th channel of the mode) offers its slots; Pan does not.
  const wheel = page.getByRole('button', { name: /^Presets of channel/ }).first();
  const address = Number(/\d+/.exec((await wheel.getAttribute('aria-label')) ?? '')![0]);
  await expect(page.getByRole('button', { name: `Presets of channel ${address - 4}`, exact: true })).toHaveCount(0);
  await wheel.click();
  await expect(page.getByRole('option', { name: /Red/ })).toBeVisible();
  await page.getByRole('option', { name: /Red/ }).click();

  // "Red" is 10-19: the channel goes to the middle of it, as scenes do.
  await expect(page.getByLabel(`Channel ${address}`, { exact: true })).toHaveValue('14');
  await expect
    .poll(async () => (await (await page.request.get('/api/runtime')).json()).state.channels)
    .toContainEqual({ universe: 1, address, value: 14 });
  // The strip names the slot now selected, and the list highlights it.
  await expect(page.locator('.strip.set .ch', { hasText: 'Red' }).first()).toBeVisible();
  await wheel.click();
  await expect(page.getByRole('option', { name: /Red/ })).toHaveAttribute('aria-selected', 'true');
  // Regression: focus went to the first preset instead of the current one.
  await expect(page.getByRole('option', { name: /Red/ })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('listbox')).toHaveCount(0);

  // Regression: opened upwards (button low in the window), the list covered
  // the button that opened it.
  await page.setViewportSize({ width: 1300, height: 320 });
  await wheel.scrollIntoViewIfNeeded();
  await wheel.click();
  const button = (await wheel.boundingBox())!;
  const list = (await page.getByRole('listbox').boundingBox())!;
  expect(button.y + button.height).toBeGreaterThan(320 * 0.55);
  expect(list.y + list.height).toBeLessThanOrEqual(button.y + 0.5);
  await page.keyboard.press('Escape');
});

test('a color wheel fixture offers its wheel colors instead of a free picker', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Fixtures', exact: true }).click();
  await page.getByLabel('Manufacturer').selectOption('Test');
  await page.getByLabel('Model').selectOption('Test/Spot 540');
  await page.getByPlaceholder('Name (default: model)').fill('Wheel spot');
  await page.getByRole('button', { name: 'Patch', exact: true }).click();

  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  await page.getByLabel('Scene target').selectOption({ label: 'Wheel spot' });
  // No palette exists yet, so none is offered (it would be an empty list).
  await expect(page.getByLabel('Scene attribute').locator('option')).toHaveText([
    'Intensity', 'Color', 'Position', 'Gobo', 'Prism', 'Zoom', 'Strobe',
  ]);
  await page.getByLabel('Scene attribute').selectOption('color');

  // The slots of the wheel, no free color input.
  const wheel = page.getByRole('radiogroup', { name: 'Color wheel colors' });
  for (const name of ['White', 'Red', 'Green', 'Blue', 'Amber']) {
    await expect(wheel.getByRole('radio', { name, exact: true })).toBeVisible();
  }
  await expect(wheel.getByRole('radio')).toHaveCount(5);
  await expect(page.locator('input[type=color]')).toHaveCount(0);

  await wheel.getByRole('radio', { name: 'Blue', exact: true }).click();
  await expect(wheel.getByRole('radio', { name: 'Blue', exact: true })).toHaveAttribute('aria-checked', 'true');
  await page.getByRole('button', { name: 'Add to scene' }).click();
  await expect(page.locator('td', { hasText: 'Color #0000ff' })).toBeVisible();

  // The engine puts the wheel on that slot: Blue is 30-39, so 34.
  const fader = page.getByLabel('Wheel spot: Color Wheel value', { exact: true });
  await expect(fader).toHaveValue('34');

  // The channel still offers its slots, as in the Simple Desk.
  await page.getByRole('button', { name: 'Presets of Wheel spot: Color Wheel', exact: true }).click();
  await page.getByRole('option', { name: /Red/ }).click();
  await expect(fader).toHaveValue('14');
  // One byte of a 16-bit channel set by hand: the scene drives the other too.
  const finePan = page.locator('.strip', { has: page.getByLabel('Wheel spot: Pan Fine value', { exact: true }) });
  await expect(finePan).toHaveClass(/\bunset\b/);
  const pan = page.getByLabel('Wheel spot: Pan value', { exact: true });
  await pan.fill('100');
  await pan.press('Enter');
  await expect(finePan).toHaveClass(/\bpaired\b/);
  await expect(finePan.getByRole('button', { name: /^Release/ })).toBeDisabled();
  // Regression: the dashed border removed the thick one that marks where a
  // fixture starts. Pan is its first channel: paired once only Pan Fine is set.
  const finePanValue = page.getByLabel('Wheel spot: Pan Fine value', { exact: true });
  await finePanValue.fill('7');
  await finePanValue.press('Enter');
  await expect(finePan).toHaveClass(/\bset\b/);
  await page.getByRole('button', { name: 'Release Wheel spot: Pan', exact: true }).click();
  const coarsePan = page.locator('.strip', { has: pan });
  await expect(coarsePan).toHaveClass(/\bpaired\b/);
  await expect(coarsePan).toHaveCSS('border-left-width', '3px');
  await expect(coarsePan).toHaveCSS('border-top-style', 'dashed');
  // One row of faders, scrolling sideways, that does not widen the page.
  const strips = page.getByRole('group', { name: 'Scene channels' });
  await page.setViewportSize({ width: 950, height: 720 });
  expect(await strips.evaluate((e) => e.scrollWidth > e.clientWidth)).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  const tops = await strips.locator('.strip').evaluateAll((all) => new Set(all.map((e) => e.getBoundingClientRect().top)).size);
  expect(tops).toBe(1);
  await page.setViewportSize({ width: 1280, height: 720 });

  // Once a palette is stored (Simple Desk), the editor offers it.
  await page.getByRole('button', { name: 'Simple Desk' }).click();
  await page.getByRole('button', { name: 'Wheel spot', exact: true }).click();
  await page.getByRole('button', { name: 'Full' }).click();
  await page.locator('select').filter({ has: page.locator('option', { hasText: 'Beam' }) }).selectOption('Intensity');
  page.once('dialog', (d) => d.accept('Bright'));
  await page.getByRole('button', { name: 'New palette' }).click();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('button', { name: 'New scene', exact: true }).click();
  // Regression: with palettes in the show the editor opened on "Palette".
  await expect(page.getByLabel('Scene attribute')).toHaveValue('intensity');
  await page.getByLabel('Scene target').selectOption({ label: 'Wheel spot' });
  await page.getByLabel('Scene attribute').selectOption('palette');
  await expect(page.locator('select option', { hasText: 'Bright (Intensity)' })).toBeAttached();
});

test('chaser steps are picked in a dialog and can be reordered', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const name = page.locator('.editor input').first();
  for (const scene of ['Step A', 'Step B', 'Step C']) {
    await page.getByRole('button', { name: 'New scene', exact: true }).click();
    await expect(name).toHaveValue(/^Scene \d+$/);
    await name.fill(scene);
    await name.press('Enter');
    await expect(page.getByRole('treeitem', { name: new RegExp(scene) })).toBeVisible();
  }

  // A new chaser opens the dialog; scenes are added in the order of the ticks.
  await page.getByRole('button', { name: 'New chaser', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  // Chasers and cue lists cannot be steps.
  await expect(dialog.getByRole('treeitem', { name: /Chasers|Cue lists/ })).toHaveCount(0);
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByLabel('Select Step A').check();
  // Steps are ordered on the chaser screen only.
  await expect(dialog.getByRole('button', { name: /^Move / })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(dialog).toHaveCount(0);
  const steps = page.getByRole('list', { name: 'Chaser steps' }).locator('li .name');
  await expect(steps).toHaveText(['Step B', 'Step A']);

  // Reopened, it appends: a scene can appear more than once.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel('Select Step C').check();
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(steps).toHaveText(['Step B', 'Step A', 'Step C', 'Step B']);

  // Step and fade are picked from a list of beat lengths (the fade also 0).
  const step = page.getByLabel('Step', { exact: true });
  const fade = page.getByLabel('Fade', { exact: true });
  const lengths = ['1/32', '1/16', '1/8', '1/4', '1/2'].map((n) => `${n} beat`).concat('1 beat', ['2', '4', '8', '16', '32'].map((n) => `${n} beats`));
  await expect(step.locator('option')).toHaveText(lengths);
  await expect(fade.locator('option')).toHaveText(['0', ...lengths]);
  await expect(step.locator('option:checked')).toHaveText('1 beat');
  await expect(fade.locator('option:checked')).toHaveText('0');
  await fade.selectOption({ label: '1/4 beat' });
  await expect(fade.locator('option:checked')).toHaveText('1/4 beat');
  await fade.selectOption({ label: '0' });

  // Cancel changes nothing.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel('Select Step A').check();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(steps).toHaveCount(4);

  // Movement and matrix effects can be steps too; their rows name the type.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel(/^Select Movement \d+$/).first().check();
  await dialog.getByLabel(/^Select Matrix \d+$/).first().check();
  await dialog.getByRole('button', { name: 'Add 2 steps' }).click();
  await expect(steps).toHaveCount(6);
  await expect(steps.nth(4)).toHaveText(/^Movement \d+$/);
  const rows = page.getByRole('list', { name: 'Chaser steps' }).locator('li');
  await expect(rows.nth(4).locator('.type')).toHaveText('Movement');
  await expect(rows.nth(5).locator('.type')).toHaveText('Matrix');
  // Without a color of their own, functions show an icon of their type.
  await expect(rows.nth(0).locator('[data-icon]')).toHaveAttribute('data-icon', 'scene');
  await expect(rows.nth(4).locator('[data-icon]')).toHaveAttribute('data-icon', 'movement_effect');
  await expect(rows.nth(5).locator('[data-icon]')).toHaveAttribute('data-icon', 'matrix_effect');
  await expect(page.getByText('effects start and stop at once')).toBeVisible();
  // The function tree of the screen shows the same icons.
  for (const [name, icon] of [[/Step A/, 'scene'], [/^Chaser \d+/, 'chaser'], [/^Movement \d+/, 'movement_effect']] as const) {
    await expect(page.getByRole('treeitem', { name }).first().locator('[data-icon]')).toHaveAttribute('data-icon', icon);
  }
  await page.getByRole('button', { name: 'Remove step 6' }).click();
  await expect(steps).toHaveCount(5);
  await page.getByRole('button', { name: 'Remove step 5' }).click();
  await expect(steps).toHaveCount(4);

  // Order: arrows, drag and drop, removal.
  await page.getByRole('button', { name: 'Move step 3 up' }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A', 'Step B']);
  await page.getByRole('button', { name: 'Remove step 4' }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);
  await steps.nth(2).dragTo(steps.nth(0));
  await expect(steps).toHaveText(['Step A', 'Step B', 'Step C']);
  await steps.nth(0).dragTo(steps.nth(2));
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);

  // Regression: two clicks before the engine answers carried the same position,
  // and the second one hit whatever had moved there. Both clicks are sent in the
  // same task. The warning shows once the second one has been handled: only then
  // is the list final.
  const warning = page.getByText('The steps changed in the meantime');
  const clickTwice = async (button: string) => {
    await page.getByRole('button', { name: button, exact: true }).evaluate((b: HTMLButtonElement) => {
      b.click();
      b.click();
    });
    await expect(warning).toBeVisible();
  };
  await clickTwice('Move step 3 up');
  await expect(steps).toHaveText(['Step B', 'Step A', 'Step C']);
  // The next check needs a warning of its own.
  await expect(warning).toHaveCount(0);
  await clickTwice('Remove step 1');
  await expect(steps).toHaveText(['Step A', 'Step C']);
  // Back to the previous list for what follows.
  await page.getByRole('button', { name: 'Add steps…' }).click();
  await dialog.getByLabel('Select Step B').check();
  await dialog.getByRole('button', { name: 'Add 1 step' }).click();
  await expect(steps).toHaveText(['Step A', 'Step C', 'Step B']);
  await page.getByRole('button', { name: 'Move step 3 up' }).click();
  await expect(steps).toHaveText(['Step A', 'Step B', 'Step C']);
  await page.getByRole('button', { name: 'Move step 1 down' }).click();
  await expect(steps).toHaveText(['Step B', 'Step A', 'Step C']);
  await page.getByRole('button', { name: 'Move step 3 up' }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);

  // The order is the engine's, not just the page's.
  await page.reload();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).click();
  await expect(steps).toHaveText(['Step B', 'Step C', 'Step A']);
  await expect(page.getByRole('dialog')).toHaveCount(0);

  // The virtual console still builds a frame with the same dialog.
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.getByRole('button', { name: 'Frame from functions…' }).click();
  await dialog.getByLabel('Solo frame (one at a time)').check();
  await dialog.getByLabel('Frame name').fill('Steps');
  await dialog.getByLabel('Select Step A').check();
  await expect(dialog.getByRole('button', { name: 'Move Step A up' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Create 1 button' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('Steps (solo)', { exact: true })).toBeVisible();

  // A running chaser lights the button of the scene its step plays.
  const stepButton = page.locator('.w.button', { hasText: 'Step A' }).last();
  await expect(stepButton).not.toHaveClass(/\bon\b/);
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).getByRole('button', { name: 'Start' }).click();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  // It follows the steps: on, off for the other scenes, on again.
  await expect(stepButton).toHaveClass(/\bon\b/);
  await expect(stepButton).not.toHaveClass(/\bon\b/);
  await expect(stepButton).toHaveClass(/\bon\b/);
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).getByRole('button', { name: 'Stop' }).click();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await expect(stepButton).not.toHaveClass(/\bon\b/);
});

test('the function of a virtual console button is picked in a dialog', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.locator('.toolbar').getByRole('button', { name: 'Button', exact: true }).click();
  const field = page.getByRole('button', { name: /^Function: / });
  await expect(field).toHaveAccessibleName('Function: — none —');

  await field.click();
  const dialog = page.getByRole('dialog');
  const assign = dialog.getByRole('button', { name: 'Assign' });
  await expect(assign).toBeDisabled();
  // One choice only: no group boxes, and picking another replaces the first.
  await expect(dialog.getByRole('checkbox')).toHaveCount(0);
  const first = dialog.getByRole('radio', { name: 'Select Step A', exact: true });
  const second = dialog.getByRole('radio', { name: 'Select Step B', exact: true });
  await first.check();
  await second.check();
  await expect(first).not.toBeChecked();
  await assign.click();
  await expect(dialog).toHaveCount(0);
  await expect(field).toHaveAccessibleName('Function: Step B');
  // A button with the default label takes the function's name.
  await expect(page.getByLabel('Label', { exact: true })).toHaveValue('Step B');

  // Reopened, the current function is chosen; Cancel changes nothing.
  await field.click();
  await expect(second).toBeChecked();
  await expect(second).toBeFocused();
  await first.check();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(field).toHaveAccessibleName('Function: Step B');

  await page.getByRole('button', { name: 'Remove function' }).click();
  await expect(field).toHaveAccessibleName('Function: — none —');

  // A widget bound to a chaser or an effect edits its times; a scene has none.
  const choose = async (kind: string) => {
    await field.click();
    await dialog.getByRole('radio', { name: new RegExp(`^Select ${kind}`) }).first().check();
    await assign.click();
    // The panel follows once the engine has the new function.
    await expect(field).toHaveAccessibleName(new RegExp(`^Function: ${kind}`));
  };
  const step = page.getByLabel('Step', { exact: true });
  const cycle = page.getByLabel('Cycle', { exact: true });
  await choose('Step A');
  await expect(step).toHaveCount(0);
  await expect(cycle).toHaveCount(0);
  await choose('Movement');
  await cycle.selectOption({ label: '16 beats' });
  await expect(cycle.locator('option:checked')).toHaveText('16 beats');
  await choose('Matrix');
  await cycle.selectOption({ label: '1/8 beat' });
  await expect(cycle.locator('option:checked')).toHaveText('1/8 beat');
  await choose('Chaser');
  await expect(cycle).toHaveCount(0);
  await step.selectOption({ label: '8 beats' });
  // Two quick edits are both kept.
  await page.getByLabel('Fade', { exact: true }).selectOption({ label: '2 beats' });
  // It is the function itself: the FX screen shows the same values.
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /^Chaser \d+/ }).click();
  await expect(step.locator('option:checked')).toHaveText('8 beats');
  await expect(page.getByLabel('Fade', { exact: true }).locator('option:checked')).toHaveText('2 beats');
  await page.getByRole('treeitem', { name: /^Movement \d+/ }).click();
  await expect(cycle.locator('option:checked')).toHaveText('16 beats');
  await page.getByRole('treeitem', { name: /^Matrix \d+/ }).click();
  await expect(cycle.locator('option:checked')).toHaveText('1/8 beat');
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await page.locator('.w.button').last().click();
  await page.getByRole('button', { name: 'Remove function' }).click();

  // Regression: a list longer than the dialog squeezed the groups, cutting off
  // and overlapping their rows, instead of scrolling.
  await page.setViewportSize({ width: 1280, height: 420 });
  await field.click();
  const tree = dialog.getByRole('tree');
  expect(await tree.evaluate((e) => e.scrollHeight > e.clientHeight)).toBe(true);
  const cut = await tree.locator('.category').evaluateAll((all) => all.filter((e) => e.scrollHeight > e.clientHeight).length);
  expect(cut).toBe(0);
  // The last row can be reached and chosen.
  const last = dialog.getByRole('radio').last();
  await last.check();
  await expect(last).toBeInViewport({ ratio: 1 });
});

test('a collection plays functions together and can be a chaser step', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  const dialog = page.getByRole('dialog');
  const name = page.locator('.editor input').first();

  // A new collection asks what it plays: scenes and effects, nothing that
  // contains other functions.
  await page.getByRole('button', { name: 'New collection', exact: true }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('treeitem', { name: /Movements/ })).toBeVisible();
  await expect(dialog.getByRole('treeitem', { name: /Chasers|Cue lists|Collections/ })).toHaveCount(0);
  await dialog.getByLabel('Select Step A', { exact: true }).check();
  await dialog.getByLabel('Select Step B', { exact: true }).check();
  await dialog.getByRole('button', { name: 'Add 2 functions' }).click();
  await expect(dialog).toHaveCount(0);
  const members = page.getByRole('list', { name: 'Collection functions' }).locator('li');
  await expect(members.locator('.name')).toHaveText(['Step A', 'Step B']);
  await name.fill('Both looks');
  await name.press('Enter');
  await expect(page.getByRole('treeitem', { name: /Both looks/ }).locator('[data-icon]')).toHaveAttribute('data-icon', 'collection');

  // Members are offered once only.
  await page.getByRole('button', { name: 'Add functions…' }).click();
  await expect(dialog.getByLabel('Select Step C', { exact: true })).toBeVisible();
  await expect(dialog.getByLabel('Select Step A', { exact: true })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Remove Step B' }).click();
  await expect(members.locator('.name')).toHaveText(['Step A']);

  // A chaser can step through it.
  await page.getByRole('button', { name: 'New chaser', exact: true }).click();
  await dialog.getByLabel('Select Both looks', { exact: true }).check();
  await dialog.getByRole('button', { name: 'Add 1 step' }).click();
  await expect(dialog).toHaveCount(0);
  const steps = page.getByRole('list', { name: 'Chaser steps' }).locator('li');
  await expect(steps.locator('.type')).toHaveText('Collection');
  await name.fill('Through collections');
  await name.press('Enter');
  await page.getByRole('treeitem', { name: /Both looks/ }).click();

  // Running, it lights the buttons of what it plays.
  await page.getByRole('treeitem', { name: /Both looks/ }).getByRole('button', { name: 'Start' }).click();
  await expect(members.first()).toHaveClass(/current/);
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  const stepButton = page.locator('.w.button', { hasText: 'Step A' }).last();
  await expect(stepButton).toHaveClass(/\bon\b/);
  await page.getByRole('button', { name: 'FX', exact: true }).click();
  await page.getByRole('treeitem', { name: /Both looks/ }).getByRole('button', { name: 'Stop' }).click();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await expect(stepButton).not.toHaveClass(/\bon\b/);
});

test('virtual console: widgets are resized by their bottom right corner', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Engine: connected')).toBeVisible();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  const toolbar = page.locator('.toolbar');
  const style = (w: Locator, key: 'left' | 'top' | 'width' | 'height') =>
    w.evaluate((el, k) => parseFloat((el as HTMLElement).style[k]), key);
  const geometry = async (w: Locator) => [await style(w, 'left'), await style(w, 'top'), await style(w, 'width'), await style(w, 'height')];
  /** Drags the corner of a widget by (dx, dy). */
  const drag = async (w: Locator, dx: number, dy: number) => {
    const handle = w.locator(':scope > .resize');
    await handle.scrollIntoViewIfNeeded();
    const box = (await handle.boundingBox())!;
    const [x, y] = [box.x + box.width - 5, box.y + box.height - 5];
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + dx, y + dy, { steps: 5 });
    await page.mouse.up();
  };

  /** Adds a widget and returns it, once it is on the screen: earlier tests left
   * widgets of the same kinds. */
  const add = async (name: string, selector: string) => {
    const all = page.locator(selector);
    const n = await all.count();
    await toolbar.getByRole('button', { name, exact: true }).click();
    await expect(all).toHaveCount(n + 1);
    return all.nth(n);
  };

  // Steps of the grid, the position unchanged.
  const knob = await add('Knob', '.w.knob');
  await expect.poll(() => geometry(knob).then((g) => g.slice(2))).toEqual([100, 130]);
  const [left, top] = await geometry(knob);
  const dial = knob.getByRole('slider');
  const before = (await dial.boundingBox())!.width;
  await drag(knob, 63, 48);
  await expect.poll(() => geometry(knob)).toEqual([left, top, 160, 180]);
  // The knob grows with its widget.
  expect((await dial.boundingBox())!.width).toBeGreaterThan(before + 30);
  // Never below the minimum, and the content still fits.
  await drag(knob, -400, -400);
  await expect.poll(() => geometry(knob)).toEqual([left, top, 40, 30]);
  await drag(knob, 80, 120);
  await expect.poll(() => geometry(knob)).toEqual([left, top, 120, 150]);

  // A button inside a frame: neither the frame moves nor changes.
  const frame = await add('Frame', '.w.frame');
  await expect.poll(() => geometry(frame).then((g) => g.slice(2))).toEqual([360, 240]);
  const framed = await geometry(frame);
  await toolbar.getByRole('button', { name: 'Button', exact: true }).click();
  const button = frame.locator('.w.button');
  await expect(button).toHaveCount(1);
  await drag(button, 50, 20);
  await expect.poll(() => geometry(button)).toEqual([10, 24, 150, 90]);
  expect(await geometry(frame)).toEqual(framed);
  // The frame is resized by its own corner, its content staying where it is.
  await drag(frame, -40, 30);
  await expect.poll(() => geometry(frame)).toEqual([framed[0], framed[1], 320, 270]);
  expect(await geometry(button)).toEqual([10, 24, 150, 90]);

  // The settings panel and the engine have the new size.
  await expect(page.getByLabel('W', { exact: true })).toHaveValue('320');
  await page.reload();
  await page.getByRole('button', { name: 'Virtual Console' }).click();
  await expect.poll(() => geometry(page.locator('.w.frame').last())).toEqual([framed[0], framed[1], 320, 270]);

  // Regression: a change made before the engine had confirmed the previous one
  // started from the old size, and saved over it. All the events are sent in
  // the same task, before any confirmation can arrive.
  const last = page.locator('.w.frame').last();
  await last.evaluate((frame) => {
    const drag = (w: Element, dx: number, dy: number) => {
      const handle = w.querySelector(':scope > .resize')!;
      const send = (type: string, x: number, y: number) =>
        handle.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 1, button: 0, clientX: x, clientY: y }));
      send('pointerdown', 0, 0);
      send('pointermove', dx, dy);
      send('pointerup', dx, dy);
    };
    const inner = frame.querySelector('.w.button')!;
    drag(inner, 30, 0);
    drag(inner, 0, 20);
    drag(frame, 20, 0);
  });
  // A second page shows what the engine has, with no preview of its own.
  const other = await page.context().newPage();
  await other.goto('/');
  await other.getByRole('button', { name: 'Virtual Console' }).click();
  const there = other.locator('.w.frame').last();
  await expect.poll(() => geometry(there)).toEqual([framed[0], framed[1], 340, 270]);
  await expect.poll(() => geometry(there.locator('.w.button'))).toEqual([10, 24, 180, 110]);
  await other.close();

  // Live, in Show mode, there is nothing to drag.
  await expect(page.locator('.w .resize').first()).toBeVisible();
  await page.getByRole('button', { name: 'Design mode' }).click();
  await expect(page.locator('.w .resize')).toHaveCount(0);
  page.once('dialog', (d) => d.accept(''));
  await page.getByRole('button', { name: 'SHOW mode' }).click();
  await expect(page.locator('.w .resize').first()).toBeVisible();
});
