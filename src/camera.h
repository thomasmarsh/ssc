#ifndef SSC_CAMERA_H
#define SSC_CAMERA_H

#include "coord.h"

class Camera {
public:
    enum Mode {
        CAMERA_OVERHEAD,
        CAMERA_FIXED_DIST,
        CAMERA_FIXED_LOC,
        CAMERA_OVERHEAD_HIGH,
        CAMERA_FPS
    };

    Camera();

    void apply();
    void update(double dt);
    void cycle();

    void set(double px, double py, double pz, // position
        double vx, double vy, double vz, // view
        double ux, double uy, double uz); // up

    // rotates the camera's view around the position using
    // axis-angle rotation
    void rotate(double angle, double x, double y, double z);

    // rotate the camera around a point (the player)
    void rotateAroundPoint(Coord3<double>& center,
        double angle, double x, double y, double z);

    void move(double speed);

    Coord3<double> mPosition, mView, mUp, mTarget;

    void setTarget(Coord3<double>& target,
        double rot,
        double speed)
    {
        mTarget.set(target);
        mTargetRotation = rot;
        mTargetSpeed = speed;
    }

    void smoothCameraMove(Coord3<double>& pos,
        Coord3<double>& view,
        Coord3<double>& up,
        double dt);

private:
    Coord3<double> constDist(double offset);

    double mFixedDistance;
    double mFixedDistVOffset;
    Mode mMode;
    double mTargetRotation;
    double mTargetSpeed;
};

#endif // SSC_CAMERA_H
