import zlib,struct,sys
F={'L':["X....","X....","X....","X....","X....","X....","XXXXX"],'A':[".XXX.","X...X","X...X","XXXXX","X...X","X...X","X...X"],
'U':["X...X"]*6+[".XXX."],'N':["X...X","XX..X","X.X.X","X..XX","X...X","X...X","X...X"],'C':[".XXXX","X....","X....","X....","X....","X....",".XXXX"],
'H':["X...X","X...X","X...X","XXXXX","X...X","X...X","X...X"],'O':[".XXX."]+["X...X"]*5+[".XXX."],'V':["X...X"]*5+[".X.X.","..X.."],
'1':["..X..",".XX..","..X..","..X..","..X..","..X..",".XXX."],'4':["...X.","..XX.","XX.X.","X..X.","XXXXX","...X.","...X."],' ':["....."]*7}
t="LAUNCH NOV 14";S=12;W=(len(t)*6+1)*S;H=9*S
rows=[]
for y in range(H):
    r=bytearray([0])
    for x in range(W):
        cx,cy=x//S-1,y//S-1;ci,cc=divmod(cx,6);v=255
        if 0<=cy<7 and 0<=ci<len(t) and cc<5 and F[t[ci]][cy][cc]=='X':v=0
        r.append(v)
    rows.append(bytes(r))
def ch(k,d):c=struct.pack(">I",len(d))+k+d;return c+struct.pack(">I",zlib.crc32(k+d))
open(sys.argv[1],'wb').write(b'\x89PNG\r\n\x1a\n'+ch(b'IHDR',struct.pack(">IIBBBBB",W,H,8,0,0,0,0))+ch(b'IDAT',zlib.compress(b''.join(rows)))+ch(b'IEND',b''))
print(W,H)
