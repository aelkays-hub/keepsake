"""airports.tsv from OurAirports' airports.csv and countries.csv (The
Unlicense): airports with an IATA code, as
ICAO \\t IATA \\t country \\t lat \\t lon \\t name."""
import csv, sys

src, out = sys.argv[1], sys.argv[2]
countries = {r["code"]: r["name"] for r in csv.DictReader(open(src + "/countries.csv", encoding="utf-8", newline=""))}
rows = []
seen = set()
for r in csv.DictReader(open(src + "/airports.csv", encoding="utf-8", newline="")):
    iata = (r.get("iata_code") or "").strip().upper()
    if len(iata) != 3 or r["type"] in ("closed", "heliport", "balloonport", "seaplane_base"):
        continue
    icao = ((r.get("icao_code") or "").strip() or (r.get("gps_code") or "").strip() or r["ident"].strip()).upper()
    if len(icao) != 4 or not icao.isalnum() or icao in seen:
        continue
    seen.add(icao)
    name = r["name"].replace("\t", " ").strip()
    country = countries.get(r["iso_country"], r["iso_country"])
    rows.append((icao, iata, country, f"{float(r['latitude_deg']):.3f}", f"{float(r['longitude_deg']):.3f}", name))
rows.sort()
with open(out, "w", encoding="utf-8", newline="\n") as f:
    f.write("# Airports with an IATA code, from OurAirports (https://ourairports.com/data/,\n")
    f.write("# The Unlicense: public domain), made by scripts/make-airports.py.\n")
    f.write("# ICAO\tIATA\tcountry\tlatitude\tlongitude\tname\n")
    for r in rows:
        f.write("\t".join(r) + "\n")
print(len(rows), "airports")
