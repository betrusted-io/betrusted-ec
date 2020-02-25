/// PDS (platform data set) is SiLab's solution to configuring the wifi chip.
/// Options such as GPIO, PA power, number of antennae are configured with PDS.
/// The data set here is "compiled" using a script (wfx-pds-compress.py) that
/// is forked from the SiLabs reference:
/// https://github.com/SiliconLabs/wfx-fullMAC-tools/blob/80e058089aa788cd8546363733890d87ab81a1c6/Tools/pds_compress
/// The fork adds this Rust data structure compatibility.

pub const PDS_DATA: [&[u8]; 7] = [
    b"{a:{a:4,b:0}}\0", 
    b"{b:{a:{a:4,b:0,c:0,d:0,e:A},b:{a:4,b:0,c:0,d:0,e:B},c:{a:4,b:0,c:0,d:0,e:C},d:{a:4,b:0,c:0,d:0,e:D},e:{a:4,b:0,c:0,d:0,e:E},f:{a:4,b:0,c:0,d:0,e:F},g:{a:4,b:0,c:0,d:0,e:G},h:{a:4,b:0,c:0,d:0,e:H},i:{a:4,b:0,c:0,d:0,e:I},j:{a:4,b:0,c:0,d:0,e:J},k:{a:4,b:0,c:0,d:0,e:K},l:{a:4,b:0,c:0,d:1,e:L},m:{a:4,b:0,c:0,d:1,e:M}}}\0", 
    b"{c:{a:{a:4},b:{a:4},c:{a:6,c:0},d:{a:3},e:{a:3},f:{a:3}}}\0", 
    b"{e:{a:{a:3,b:26,c:26},b:0,c:0}}\0", 
    b"{f:{c:{c:0,b:0,d:0,a:0,f:0,e:0},b:{a:10,b:D,c:82,d:5,e:0,f:0}}}\0", 
    b"{h:{e:0,a:50,b:0,d:0,c:[{a:1,b:[0,0,0,0,0,0]},{a:2,b:[0,0,0,0,0,0]},{a:[3,9],b:[0,0,0,0,0,0]},{a:A,b:[0,0,0,0,0,0]},{a:B,b:[0,0,0,0,0,0]},{a:[C,D],b:[0,0,0,0,0,0]},{a:E,b:[0,0,0,0,0,0]}]}}\0", 
    b"{j:{a:0,b:0}}\0", 
];
