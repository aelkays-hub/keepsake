import { describe, expect, it } from "vitest";

import { groupName, tagName } from "../metaNames";

describe("metadata names", () => {
  it("names ExifTool groups for people", () => {
    expect(groupName("IFD0")).toBe("Camera and image (IFD0)");
    expect(groupName("ExifIFD")).toBe("Exposure (EXIF)");
    expect(groupName("XMP-dc")).toBe("XMP (dc)");
    expect(groupName("Sony")).toBe("Sony maker notes");
    expect(groupName("IPTC")).toBe("IPTC");
    expect(groupName("ICC-header")).toBe("ICC-header");
    expect(groupName("SR2DataIFD3")).toBe("SR2DataIFD3");
    expect(groupName("PrintIM")).toBe("PrintIM");
  });

  it("splits tag names into words, keeping acronyms", () => {
    expect(tagName("ExposureTime")).toBe("Exposure Time");
    expect(tagName("ISOSetting")).toBe("ISO Setting");
    expect(tagName("ISO")).toBe("ISO");
    expect(tagName("GPSLatitude")).toBe("GPS Latitude");
    expect(tagName("FNumber")).toBe("F Number");
    expect(tagName("Make")).toBe("Make");
  });
});
