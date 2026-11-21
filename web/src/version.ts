declare const __XBNC_VERSION__: string;

/** The xBNC version this panel was built with (the binary's version). */
export const VERSION: string = __XBNC_VERSION__;

/** "xBNC v1.0.0 - https://xbnc.org" (the version comes from Cargo.toml). */
export const BRAND = `xBNC v${VERSION} - https://xbnc.org`;
