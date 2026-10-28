# ROS 2 の型の取り込みのテスト用 (issue #201)
# 2 行目の説明

uint8 MODE_A=1
string NAME_PREFIX = demo_
float64 SCALE=0.5

# 時刻と座標系
std_msgs/Header header
int16 small -5  # 行末のコメント
uint32 count
uint64 big
string<=10 label "abc"
int32[3] fixed [1, 2, 3]
float64[<=4] bounded
uint8[] blob
uint8[2] pair
geometry_msgs/Pose pose
Inner[] inners
wstring wide
bool flag true
char letter
