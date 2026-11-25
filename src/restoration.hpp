#pragma once
#include <string>

namespace dme {
inline std::string regional_chain(){
    // Thresholded smoothing: low-contrast areas are filtered more than edges.
    return "format=yuv444p,smartblur=lr=1.2:ls=0.65:lt=8:cr=0.8:cs=0.4:ct=6";
}
inline std::string deblur_chain(){
    // Damped first-order approximate inverse of the 3x3 binomial blur kernel.
    // I + 0.5*(I-K); DC gain = 1. A second, wider high-boost stage is mild.
    return "format=yuv444p,convolution=0m='-1 -2 -1 -2 44 -2 -1 -2 -1':0rdiv=0.03125,unsharp=7:7:0.12:5:5:0";
}
inline std::string wavelet_chain(){
    return "format=yuv444p,vaguedenoiser=threshold=2:method=garrote:nsteps=3:percent=75:planes=1:type=universal";
}
inline std::string resize_chain(const std::string& factor){
    return "scale=iw*"+factor+":ih*"+factor+":flags=lanczos+accurate_rnd+bitexact";
}
}
