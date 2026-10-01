"""Record 0131: the probe's fixed texts. Written once by `write`; read by the
twin, the rnx path and the reference, so every side sees the same bytes."""
import csv, json, pathlib, sys

DOCS = [
    # accounts and passwords
    "To reset your password, open Settings, choose Security and select Reset password; we email a link that expires in one hour.",
    "If the password reset email never arrives, check your spam folder and confirm the address on your account is current.",
    "Two-factor authentication adds a one-time code from your phone each time you sign in from a new device.",
    "You can change the email address on your account under Settings, then Profile; we confirm the new address before switching.",
    "Accounts are locked for fifteen minutes after five failed sign-in attempts to protect against password guessing.",
    "To delete your account permanently, contact support; deletion removes your data after a thirty-day grace period.",
    "Recovery codes let you sign in if you lose your phone; store them somewhere safe and offline.",
    "Single sign-on is available for business plans through SAML and OpenID Connect providers.",
    "Usernames can be changed once every ninety days from the Profile page.",
    "Sessions on other devices can be signed out remotely from the Security page.",
    # billing
    "Invoices are emailed on the first business day of each month and are also available under Billing, then History.",
    "We accept major credit cards, bank transfers for annual plans, and PayPal in supported countries.",
    "To update the card used for payments, open Billing, choose Payment methods and add the new card before removing the old one.",
    "A failed payment is retried three times over seven days before the subscription is paused.",
    "Refunds for annual plans are prorated if requested within the first thirty days.",
    "Sales tax or VAT is added at checkout based on the billing address you provide.",
    "Switching from monthly to annual billing applies a credit for the unused part of the current month.",
    "Discount codes must be entered at checkout and cannot be applied to past invoices.",
    "Business customers can request purchase orders and net-thirty payment terms from the sales team.",
    "Your plan renews automatically unless you cancel before the renewal date shown on the Billing page.",
    # shipping
    "Standard shipping takes three to five business days; express shipping arrives in one to two business days.",
    "A tracking number is emailed as soon as your order leaves the warehouse.",
    "We ship to most countries; customs duties for international orders are paid by the recipient.",
    "Orders placed before two in the afternoon on a business day ship the same day.",
    "If a package is marked delivered but missing, check with neighbours and wait one day before contacting us.",
    "You can change the delivery address until the order is packed; after that, contact the carrier directly.",
    "Large items are delivered by freight and require an appointment with the delivery company.",
    "Shipping is free on orders over fifty dollars within the continental United States.",
    "Damaged packages should be photographed before opening so we can file a claim with the carrier.",
    "Orders to post office boxes are sent by postal service rather than courier.",
    # returns
    "Items can be returned within thirty days of delivery in their original condition and packaging.",
    "Start a return from Orders, choose the item and print the prepaid label we provide.",
    "Refunds are issued to the original payment method within five business days of receiving the return.",
    "Final sale items, gift cards and opened software cannot be returned.",
    "Exchanges for a different size are free; we ship the replacement as soon as the return is scanned.",
    "If you received the wrong item, we send the correct one immediately and you keep the label to return the mistake.",
    "Return shipping is free for defective items and deducted from the refund otherwise.",
    "Gifts can be returned for store credit without notifying the person who sent them.",
    "Warranty claims for defects after thirty days are handled by the manufacturer.",
    "Refurbished items carry a ninety-day return window instead of thirty days.",
    # privacy and data
    "You can download a copy of all your data from Settings, then Privacy, then Export.",
    "We never sell personal information; data is shared only with processors needed to run the service.",
    "Cookies for analytics can be turned off in the cookie preferences at the bottom of any page.",
    "Data is stored in encrypted form in data centres located in the region you chose at sign-up.",
    "To correct inaccurate personal data, edit your profile or contact our privacy team.",
    "Marketing emails include an unsubscribe link, and preferences can be changed under Notifications.",
    "Access logs are kept for ninety days for security purposes and then deleted.",
    "Children under thirteen may not create accounts.",
    "We notify affected users within seventy-two hours of discovering a data breach.",
    "Third-party integrations only receive the permissions you approve when connecting them.",
    # technical
    "If the app crashes on start, update to the latest version and restart your device.",
    "Clearing the browser cache often fixes pages that load without styles or images.",
    "The desktop app requires a sixty-four-bit operating system and four gigabytes of memory.",
    "Sync conflicts are resolved by keeping both versions and labelling the older one as a copy.",
    "Our status page reports outages and scheduled maintenance in real time.",
    "API rate limits are one thousand requests per minute per token on paid plans.",
    "Offline mode keeps your recent files available and syncs changes when you reconnect.",
    "Notifications not appearing on mobile usually means they are disabled in the phone's system settings.",
    "Exports larger than two gigabytes are split into several archive files.",
    "Keyboard shortcuts are listed under Help, then Shortcuts, and can be customised.",
]
QUERIES = [
    "how do I reset my password?",
    "I forgot my login and the email never came",
    "when will my order arrive?",
    "can I get my money back for a return?",
    "how do I change my credit card?",
    "is my personal data sold to advertisers?",
    "the app keeps crashing when I open it",
    "what happens if my payment fails?",
]
FRAGS_A = ["Please note that", "In most cases", "For business plans", "On mobile devices", "After the first month", "For international orders", "During maintenance", "When signing in"]
FRAGS_B = ["refunds", "invoices", "passwords", "deliveries", "exports", "notifications", "returns", "sessions", "receipts", "backups"]
FRAGS_C = ["are processed within five business days", "can be changed from the settings page", "may take longer than usual",
           "are sent by email", "require a confirmed account", "are limited to the account owner", "are retried automatically",
           "are kept for ninety days", "can be requested from support", "depend on the region"]


def corpus(n):
    out = []
    for i in range(n):
        a, b, c = FRAGS_A[i % 8], FRAGS_B[(i // 8) % 10], FRAGS_C[(i // 80) % 10]
        extra = " and the same applies to " + FRAGS_B[(i * 7) % 10] if i % 3 == 0 else ""
        out.append(f"{a} {b} {c}{extra} (case {i}).")
    return out


def controls():
    long_text = " ".join(["Shipping, returns and billing questions are answered by the support team every weekday."] * 40)
    return {
        "alone": [DOCS[0]],
        "mixed": [DOCS[0], "ok", long_text, "", DOCS[20]],
        "empty": [""],
        "whitespace": ["   \t "],
        "long": [long_text],
        "thirty_three": DOCS[:33],
    }


def write(d):
    d = pathlib.Path(d)
    d.mkdir(parents=True, exist_ok=True)
    with open(d / "docs.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["id", "text"])
        for i, t in enumerate(DOCS):
            w.writerow([i, t])
    with open(d / "corpus.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["id", "text"])
        for i, t in enumerate(corpus(10_000)):
            w.writerow([i, t])
    with open(d / "queries.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["query"])
        for q in QUERIES:
            w.writerow([q])
    (d / "texts.json").write_text(json.dumps({"docs": DOCS, "queries": QUERIES, "controls": controls(), "corpus": corpus(10_000)}))


if __name__ == "__main__":
    assert len(DOCS) == 60 and len(QUERIES) == 8
    write(sys.argv[1])
