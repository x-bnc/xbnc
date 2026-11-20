#!/usr/bin/env python3
"""Checks every way of changing the panel language, in both directions.

    pip install playwright && playwright install chromium
    python3 testenv/ui/language_check.py https://bnc.example.org:8443 USER PASSWORD

Use a superadministrator (the Settings page is checked too). The script
changes the account and server languages; set them back afterwards if needed.
Exit code 0: everything switched; 1: something stayed in the old language.
"""
import re
import sys

from playwright.sync_api import sync_playwright

if len(sys.argv) != 4:
    sys.exit(__doc__)
BASE, USER, PASSWORD = sys.argv[1].rstrip("/"), sys.argv[2], sys.argv[3]
WORD = {"English": "Home", "Español": "Inicio"}  # first menu entry
failures = []


def menu(pg):
    pg.wait_for_timeout(1200)
    return pg.locator("nav").first.inner_text()


def expect(pg, lang, step):
    ok = WORD[lang] in menu(pg)
    print(("ok   " if ok else "FAIL ") + step + " -> " + lang)
    if not ok:
        failures.append(step + " -> " + lang)


def login(browser, pick):
    ctx = browser.new_context(viewport={"width": 1360, "height": 900}, ignore_https_errors=True)
    pg = ctx.new_page()
    pg.goto(BASE + "/")
    pg.wait_for_timeout(1000)
    pg.get_by_role("button", name=pick).click()
    pg.locator("input").first.fill(USER)
    pg.locator("input[type=password]").first.fill(PASSWORD)
    pg.get_by_role("button", name=re.compile("^(Entrar|Sign in)$")).click()
    expect(pg, pick, "sign in with " + pick)
    return ctx, pg


with sync_playwright() as p:
    browser = p.chromium.launch()
    for first, other in (("English", "Español"), ("Español", "English")):
        ctx, pg = login(browser, first)
        pg.goto(BASE + "/profile")
        pg.get_by_role("radio", name=other).first.click()
        expect(pg, other, "Your account (signed in in " + first + ")")
        pg.get_by_role("radio", name=first).first.click()
        expect(pg, first, "Your account back")
        pg.goto(BASE + "/settings")
        pg.wait_for_timeout(1200)
        pg.get_by_role("radio", name=other).first.click()
        expect(pg, other, "Settings (signed in in " + first + ")")
        pg.reload()
        expect(pg, other, "Settings, after reloading")
        ctx.close()
    browser.close()

if failures:
    print("\n%d step(s) failed" % len(failures))
    sys.exit(1)
print("\nAll language changes applied.")
