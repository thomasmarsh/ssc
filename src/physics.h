#ifndef SSC_PHYSICS_H
#define SSC_PHYSICS_H

#include "coord.h"
#include "ode/ode.h"

#include <algorithm>

//! The ERP specifies what proportion of the joint error will be fixed during
//! the next simulation step. If ERP=0 then no correcting force is applied and
//! the bodies will eventually drift apart as the simulation proceeds. If ERP=1
//! then the simulation will attempt to fix all joint error during the next
//! time step. However, setting ERP=1 is not recommended, as the joint error
//! will not be completely fixed due to various internal approximations. A
//! value of ERP=0.1 to 0.8 is recommended (0.2 is the default).

const dReal ERP = 0.8;

//! I would like to have such a nice statement as the above paragraph for
//! variable ERP, but I don't have one handy from the ODE documentation. CFM
//! should be near zero, but apparently *not equal* to zero.

const dReal CFM = 0.000001;

//! Prototype for NearCallback (implemented in physics.cc). This function is
//! called whenever two geometries are close enough to each other.

extern void NearCallback(void* data, dGeomID g1, dGeomID g2);

// ---------------------------------------------------------------------------
//
//! \class Environ
//
//! The environment class is responsible for containing the ODE word and space,
//! as well as the contact group for handling collisions. Its responsibilities
//! are:
//!
//!     - construction of the physics model within ODE
//!
//!     - factory methods for adding bodies and geometries
//!
//!     - interface for timestepping the world
//
// ---------------------------------------------------------------------------

class Environ {
private:
    //! the collision detection routine has full access to the environment

    friend void NearCallback(void*, dGeomID, dGeomID);

    //! \name ODE Data Types

    //@{
    //! The ODE world, space, and collision contacts

    dWorldID mWorld;
    dSpaceID mSpace;
    dJointGroupID mContactGroup;

    //@}

    //! static instance

    //! private constructor

    Environ(bool useQuadTree = false)
        : mWorld(0)
        , mSpace(0)
        , mContactGroup(0)
    {
        mWorld = dWorldCreate();

        // there is a QuadTree space available in ODE, but I couldn't
        // get it to work any more efficiently than the hash space,
        // so by default it is not used.
        if (useQuadTree) {
            dVector3 center = { 1500, 1500, 0 },
                     extents = { 10, 0, 10, 0 };
            mSpace = dQuadTreeSpaceCreate(0, center, extents, 7);
        } else {
            mSpace = dHashSpaceCreate(0);
        }

        mContactGroup = dJointGroupCreate(0);

        // no gravity in our universe
        dWorldSetGravity(mWorld, 0, 0, 0);

        dWorldSetERP(mWorld, ERP);
        dWorldSetCFM(mWorld, CFM);
    }

public:
    //! there can be only one world at a time; this method takes the place
    //! of the constructor.

    static Environ& getInstance()
    {
        static Environ instance;
        return instance;
    }

    //! \name Factory Methods

    //@{
    //! The following methods are factory methods for creating ODE
    //! bodies and geometries of different shapes

    inline dBodyID newBody() { return dBodyCreate(mWorld); }
    inline dGeomID newSphere(double r) { return dCreateSphere(0, r); }
    inline dGeomID newPlane(double a, double b, double c, double d)
    {
        return dCreatePlane(0, a, b, c, d);
    }
    inline dGeomID newRay(double len) { return dCreateRay(0, len); }

    inline dJointGroupID newJointGroup() { return dJointGroupCreate(0); }
    inline dJointID newHinge(dJointGroupID group)
    {
        return dJointCreateHinge(mWorld, group);
    }

    //@}

    //! Once a geometry has been created, setSpace must be called on
    //! it. For some reason the following example does not work:
    //!
    //! \code
    //!     dGeomID sphere = dCreateSphere(mSpace, 10);
    //! \endcode
    //!
    //! while the following does work:
    //!
    //! \code
    //!     dGeomID sphere = dCreateSphere(0, 10);
    //!     dSpaceAdd(mSpace, sphere)
    //! \endcode

    inline void setSpace(dGeomID g) { dSpaceAdd(mSpace, g); }

    //! Collision detection

    //! This is the main timestep function; it is called once per
    //! frame within the game loop. As with any ODE simulation,
    //! the process is:
    //!
    //!     - check for all overlapping objects (and calculate
    //!       response by adding contact joints)
    //!
    //!     - step the world by timestep dt
    //!
    //!     - clear the contact joints, and repeat forever
    //!
    //! Notably, we are calling dWorldStepFast1() with 5 iterations. This
    //! is *much* faster than the regular dWorldStep(), but sacrifices
    //! accuracy. I've not had any accuracy problems, and it is not likely
    //! important in a game like SSC.

    inline void update(double dt)
    {
        dSpaceCollide(mSpace, 0, NearCallback);
        //dWorldStepFast1(mWorld, dt, 5);
        dWorldStep(mWorld, dt); // , 5)
        dJointGroupEmpty(mContactGroup);
    }
};

// ---------------------------------------------------------------------------
//
//! \enum CollisionObjectType
//
//! Collision data is attached to each object and helps to determine the
//! appropriate response to a collision. The collision types available
//! are:
//!
//!     - wall
//!     - screen object
//!     - ray
//!
//! Walls are only used for the edge of the gameplay area. ScreenObject's
//! correspond to most entities (the player, bogeys, asteroids, bullets, etc).
//! Rays are only used by the bogeys for collision avoidance (like eyes).

enum CollisionObjectType {
    COLLISION_WALL,
    COLLISION_SCREENOBJECT,
    COLLISION_RAY
};

//! \struct CollisionData
//
//! The structure which uses enum #CollisionObjectType will contain a
//! pointer to some user data. Usually this is a pointer to self, null
//! (when it can be ignored), or a pointer to some parent object.

struct CollisionData {
    CollisionData()
        : type(COLLISION_WALL)
        , ptr(0)
    {
    }
    CollisionObjectType type;
    void* ptr;
};

// ---------------------------------------------------------------------------
//
//! \class Collidable
//
//! Each of the object types mentioned above (wall, screenobject, ray) are
//! implemented as subclasses of Collidable. The only responsibilities  of
//! the Collidable class is to:
//!
//!     - maintain collision data (usually the object's type and a pointer
//!       to it self)
//!
//!     - provide a virtual shouldCollide() method to be implemented by
//!       all subclasses
//
// ---------------------------------------------------------------------------

class Collidable {
protected:
    dGeomID mGeometry;
    CollisionData mCollData;

public:
    Coord3<double> mPosition;
    Collidable()
        : mGeometry(0)
    {
    }

    virtual ~Collidable() { dGeomDestroy(mGeometry); }

    inline const CollisionData* collisionData() { return &mCollData; }

    //! this virtual method is only used by flocking types right now..
    inline virtual void avoid(Collidable*) {}

    inline virtual void enable() { dGeomEnable(mGeometry); }
    inline virtual void disable() { dGeomDisable(mGeometry); }

    void setData(CollisionObjectType t, void* data)
    {
        mCollData.type = t;
        mCollData.ptr = data;
        dGeomSetData(mGeometry, &mCollData);
    }

    inline virtual bool shouldCollide(Collidable* other) { return true; }
};

// ---------------------------------------------------------------------------
//
//! \class Wall
//
//! The wall is the simplest collidable in the game. These are only used at
//! the edge of the gameplay area, and are fixed in the static environment.
//
// ---------------------------------------------------------------------------

class Wall : public Collidable {
public:
    Wall(double a, double b, double c, double d)
    {
        Environ& environ = Environ::getInstance();

        mGeometry = environ.newPlane(a, b, c, d);
        environ.setSpace(mGeometry);
        dGeomSetData(mGeometry, 0);
    }

    virtual ~Wall() {}
};

// ---------------------------------------------------------------------------
//
//! \class Ray
//
//! The ray is the way a bogey keeps track of the environment around itself.
//! It maintains a pointer to the object on whose behalf it is instantiated
//! (e.g., a pointer to the Bogey which is using it).
//!
//! Its position in space is constantly maintained by calling its set() method.
//
// ---------------------------------------------------------------------------

class Ray : public Collidable {
private:
    Collidable* mParent;
    double radius;

public:
    Ray(Collidable* p, double r = 100)
        : mParent(p)
        , radius(r)
    {
        Environ& environ = Environ::getInstance();
        mGeometry = environ.newSphere(radius);
        setData(COLLISION_RAY, (void*)mParent);
        environ.setSpace(mGeometry);
    }
    virtual ~Ray() { mParent = 0; }

    inline virtual bool shouldCollide(Collidable* other)
    {
        mParent->avoid(other);
        return false;
    }

    inline double getLength() { return radius; }

    inline void set(double x, double y, double z)
    {
        dGeomSetPosition(mGeometry, x, y, z);
    }
};

// ---------------------------------------------------------------------------
//
//! \class PhysicsObject
//
//! This serves as the base type for most the game objects (missile, bogey,
//! ship, asteroid, etc). Physics objects maintain:
//!
//!     VARIABLES
//!     - velocity (note: mPosition is stored in Collidable)
//!     - mass
//!     - radius
//!     - speed
//!
//!     METHODS
//!     - acceleration
//!     - disable/enable from the game world
//
// ---------------------------------------------------------------------------

class PhysicsObject : public Collidable {
private:
    dBodyID mBody;

public:
    Coord3<double> mVelocity;

    double mass, radius;
    double speed;

    PhysicsObject(double r, double _mass,
                  double x, double y, double z,
                  double fx, double fy, double fz,
                  void* data)
        : mVelocity(fx, fy, fz)
        , mass(_mass)
        , radius(r)
        , speed(hypot(fx, fy))
    {
        Environ& environ = Environ::getInstance();

        // set all body params
        mBody = environ.newBody();
        dBodySetForce(mBody, fx, fy, fz);
        dBodySetLinearVel(mBody, fx, fy, fz);

        // set mass
        dMass m;
        if (radius < 0) {
            fprintf(stderr, "radius error: %g\n", radius);
        }
        if (mass < 0) {
            fprintf(stderr, "mass error: %g\n", mass);
        }

        radius = std::max(radius, 1.0);
        mass = std::max(mass, 1.0);
        dMassSetSphereTotal(&m, mass, radius);
        dBodySetMass(mBody, &m);

        // set all geometry params
        mGeometry = environ.newSphere(radius);
        environ.setSpace(mGeometry);
        dGeomSetBody(mGeometry, mBody);
        setData(COLLISION_SCREENOBJECT, data);

        // set physical position
        setPosition(x, y, z);
    }

    virtual ~PhysicsObject()
    {
        dBodyDestroy(mBody);
    }

    //! \name setPosition and setVelocity

    //@{
    //! It is not recommended (except at object initialization), but these
    //! two methods will directly set the position and velocity of the
    //! object.

    inline void setPosition(double x, double y, double z)
    {
        mPosition.set(x, y, z);
        dBodySetPosition(mBody, x, y, z);
        dGeomSetPosition(mGeometry, x, y, z);
    }

    inline void setVelocity(double x, double y, double z)
    {
        mVelocity.set(x, y, z);
        dBodySetLinearVel(mBody, x, y, z);
    }

    //@}

    //! \name Methods to enable and disable

    //@{
    //! The following methods enable and disable the game from the game
    //! world. Normally, when an object is dying or dead, a call to
    //! disable() should be made to prevent it from initiating any
    //! collisions.

    inline void disable()
    {
        Collidable::disable();
        dBodyDisable(mBody);
    }
    inline void enable()
    {
        Collidable::enable();
        dBodyEnable(mBody);
    }

    //@}

    //! We maintain a position and velocity in SSC's native Coord3<double>
    //! format. Every frame we should sync() in order to copy these
    //! values out of the ODE model.

    virtual inline void sync()
    {
        const dReal *pos = dGeomGetPosition(mGeometry),
                    *vel = dBodyGetLinearVel(mBody);

        mPosition.set(pos[0], pos[1], pos[2]);
        mVelocity.set(vel[0], vel[1], vel[2]);
        speed = mVelocity.length();
    }

    //! acceleration is achieved by adding x, y, and z forces to the
    //! object.

    inline virtual void accelerate(double fx, double fy, double fz)
    {
        dBodyAddForce(mBody, fx, fy, fz);
    }
};

#endif // SSC_PHYSICS_H
