// Current geometric upper-half-plane angle: y is cross length, x is dot.
const ATAN_COEFF:array<f64,21> = array<f64,21>(1.0lf,0.33333333333333331lf,0.20000000000000001lf,0.14285714285714285lf,0.1111111111111111lf,0.090909090909090912lf,0.076923076923076927lf,0.066666666666666666lf,0.058823529411764705lf,0.052631578947368418lf,0.047619047619047616lf,0.043478260869565216lf,0.040000000000000001lf,0.037037037037037035lf,0.034482758620689655lf,0.032258064516129031lf,0.030303030303030304lf,0.028571428571428571lf,0.027027027027027029lf,0.02564102564102564lf,0.024390243902439025lf);
fn atan_reduced(z:f64)->f64{
    let zz=z*z;
    var p=ATAN_COEFF[20];
    for(var k:i32=19;k>=0;k--){p=ATAN_COEFF[u32(k)]-zz*p;}
    return z*p;
}
fn angle_upper(y_input:f64,x:f64)->f64{
    let y=abs(y_input);
    let ax=abs(x);
    let pi:f64=3.1415926535897932384626433832795lf;
    let half:f64=1.5707963267948966192313216916398lf;
    let quarter:f64=0.78539816339744830961566084581988lf;
    if y==0.0lf && ax==0.0lf{return half;}
    let invert=y>ax;
    var r:f64;
    if invert {r=ax/y;}else{r=y/ax;}
    var z=r;var offset:f64=0.0lf;
    if r>0.4142135623730950488016887242097lf {z=(r-1.0lf)/(r+1.0lf);offset=quarter;}
    var result=offset+atan_reduced(z);
    if invert {result=half-result;}
    if x<0.0lf {result=pi-result;}
    return result;
}
