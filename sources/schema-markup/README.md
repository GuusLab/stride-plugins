# Schema Markup

Adds JSON-LD structured data to every published page, so search engines can show rich results for your business and your posts. It adds one `<script type="application/ld+json">` to the page head. Visitors do not see it.

## What it adds

A schema.org `@graph` with:

- **Your organization**: an `Organization`, `LocalBusiness`, `Store`, `Restaurant`, `ProfessionalService` or `Person`, with name, logo, phone, email, postal address, social profiles (`sameAs`) and, for local businesses, `openingHoursSpecification` and `priceRange`.
- **`WebSite`**: name, address and language, linked to the organization as its publisher.
- **`BlogPosting`** for posts: headline (the page's `<h1>`), description, social image, language, word count, author and publisher. It also includes `datePublished` when the page has an `article:published_time` meta tag or a `<time datetime>`.

The plugin reads everything it can from the page itself: the `<title>`, the `<h1>`, the meta description, `og:image`, the canonical link and `<html lang>`. User-supplied text goes into the output as JSON. The characters `<`, `>` and `&` are escaped as `<`, `>` and `&`, so no value can close the script tag. A page that already carries this plugin's markup is left alone, and so is a page without a `<head>`.

## Zero configuration

With no settings saved:

- The site name comes from the page titles. The plugin takes the part after the last ` | `, ` - `, ` – ` or ` · `; on the home page, it uses the whole title.
- The site address comes from the page's canonical link. If the page has none, the output has no URLs.
- Pages whose slug starts with `blog-`, `news-`, `blog/` or `news/` are marked up as posts.

## Settings (Site settings → Schema Markup)

| Field | What it does |
| --- | --- |
| Add structured data | Turn off to leave pages untouched. |
| What you are | Organization, Local business, Shop, Restaurant or cafe, Professional service, Person. |
| Name | Overrides the name taken from the title. |
| Site address | `https://example.com`. The plugin uses it for `@id`s and to turn paths like `/media/logo.png` into full URLs. |
| Logo URL | An absolute URL, or a path when the site address is set. |
| Phone, Email | `telephone` and `email`. |
| Street, Postal code, City, Country | The `PostalAddress`. Country is a two-letter code. |
| Opening hours | One range per line, such as `Mo-Fr 09:00-17:30` or `Sa,Su 10:00-16:00`. Day lists and ranges can be combined (`Mo-We,Fr`). Used only for local business types. |
| Price range | For example `€€`. Used only for local business types. |
| Social profiles | One URL per line. |
| Posts live under | Comma-separated slug prefixes. `blog` matches `blog-…` and `blog/…`. The `blog` page itself does not count as a post. |
| Default author | Named as the `Person` author of posts. If empty, the organization is credited. |

The panel checks what you submit. It refuses to save a malformed URL, country code, phone number, email address or opening-hours line, and tells you which one is wrong.

## Permissions

- **`storage`**, which keeps the panel's settings. This is the only permission the plugin asks for. It never reads other pages or site settings, and it makes no network requests.

If storage is refused, the plugin still runs with the zero-configuration defaults above, and the panel explains why it cannot save.

## Build

```
../../tools/build-plugin.sh schema-markup   # or ./build.sh
```

Unit tests: `cargo test` (run with a throwaway `CARGO_TARGET_DIR`).
